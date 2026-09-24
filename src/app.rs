//! The applet: a panel button and the Start menu popup it opens.

use cosmic::app::{Core, Task};
use cosmic::iced::keyboard::{key::Named, Key as KeyCode};
use cosmic::iced::window::{self, Id};
use cosmic::iced::{Length, Limits, Point, Subscription};
use cosmic::widget::{column, container, mouse_area, popover, row, text, text_input};
use cosmic::{Application, Element};

use crate::apps::App as AppEntry;
use crate::config::{Config, ListMode, RightSide, TileRef, TileSize};
use crate::fl;
use crate::folders::Folder;
use crate::session::Power;
use crate::ui::{self, Spacing};

/// Rail + list + a six-cell tile column, with padding. Fixed like Windows 10's
/// menu; each column scrolls inside it.
pub const POPUP_WIDTH: f32 = 680.0;
pub const POPUP_HEIGHT: f32 = 600.0;

pub struct App {
    core: Core,
    popup: Option<Id>,
    config: Config,
    apps: Vec<AppEntry>,
    usage_top: Vec<String>,
    power_open: bool,
    error: Option<String>,
    query: String,
    /// Keyboard selection among search results.
    selected: usize,
    search_id: cosmic::widget::Id,
    /// Last pointer position over the popup, where a right-click menu opens.
    pointer: Point,
    context: Option<Context>,
    edit: ui::tiles::Edit,
    letter_grid: bool,
    list_id: cosmic::widget::Id,
    folders: Vec<Folder>,
    loose: Vec<usize>,
    open_folders: std::collections::HashSet<usize>,
    mode_menu: bool,
    /// Bumped on every config edit; a save result older than this is stale.
    edit_gen: u64,
    favs: Vec<String>,
    recent: Vec<String>,
    right_menu: bool,
    avatar: Avatar,
}

/// The account picture, or the initial to draw when there is none.
#[derive(Debug, Clone, Default)]
pub struct Avatar {
    pub image: Option<std::path::PathBuf>,
    pub initial: String,
}

/// AccountsService keeps the picture world-readable under the user's name;
/// `~/.face` is the older convention. The initial comes from the account's
/// full name, falling back to the login name.
fn load_avatar() -> Avatar {
    let user = std::env::var("USER").unwrap_or_default();
    let image = [
        std::path::PathBuf::from("/var/lib/AccountsService/icons").join(&user),
        dirs::home_dir().unwrap_or_default().join(".face"),
    ]
    .into_iter()
    .find(|p| !user.is_empty() && p.is_file());
    let full_name = std::fs::read_to_string("/etc/passwd")
        .ok()
        .and_then(|passwd| {
            passwd
                .lines()
                .find(|l| l.split(':').next() == Some(user.as_str()))
                .and_then(|l| l.split(':').nth(4).map(str::to_owned))
        })
        .unwrap_or_default();
    let initial = full_name
        .chars()
        .chain(user.chars())
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_default();
    Avatar { image, initial }
}

/// What a right-click menu is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// Index into the app list.
    App(usize),
    /// A tile, by its place in the config.
    Tile(TileRef),
}

#[derive(Debug, Clone, Copy)]
pub struct Context {
    pub target: Target,
    pub at: Point,
}

/// Keys the search handles itself. Enter comes from the input's own submit,
/// so it is not listened for here and cannot fire twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    Escape,
}

#[derive(Debug, Clone)]
pub enum Message {
    TogglePopup,
    PopupClosed(Id),
    /// Everything read from disk when the popup opens, in one go.
    Loaded(Box<Loaded>),
    Launch(usize),
    /// A tile, which knows its app by id rather than list position.
    LaunchId(String),
    LetterGrid(bool),
    PowerMenu(bool),
    Power(Power),
    ShowError(String),
    OpenSettings,
    Query(String),
    SearchKey(Key),
    Submit,
    Pointer(Point),
    OpenContext(Target),
    CloseContext,
    Pin(String),
    Unpin(String),
    Resize(TileRef, TileSize),
    MoveToGroup(TileRef, usize),
    NewGroupWith(TileRef),
    RunAction(usize, usize),
    ToggleEdit,
    JumpTo(char),
    JumpToCategory(&'static str),
    ModeMenu(bool),
    SetListMode(ListMode),
    ToggleFolder(usize),
    RightMenu(bool),
    SetRightSide(RightSide),
    AddFavourite(String),
    RemoveFavourite(String),
    FavouritesSaved(Vec<String>),
    ConfigSaved(u64, Box<Config>),
    /// A tile pressed in edit mode: pick it up, drop onto it, or put it back.
    TileClicked(TileRef),
    DropEnd(usize),
    DropNew,
    AddGroup,
    RenameGroup(usize, String),
    RemoveGroup(usize),
    OpenFiles,
    OpenSettingsApp,
    OpenAccount,
}

#[derive(Debug, Clone)]
pub struct Loaded {
    pub apps: Vec<AppEntry>,
    pub most_used: Vec<String>,
    pub config: Config,
    pub folders: Vec<Folder>,
    pub loose: Vec<usize>,
    pub favs: Vec<String>,
    pub recent: Vec<String>,
    pub avatar: Avatar,
}

/// Read the app index, launch history and config. Blocking file I/O, so it
/// runs on the blocking pool, never in `update`.
fn load() -> Loaded {
    let apps = crate::apps::load_all();
    let ids: std::collections::HashSet<&str> = apps.iter().map(|a| a.id.as_str()).collect();
    let usage = crate::usage::Usage::path()
        .map(|p| crate::usage::Usage::load_from(&p))
        .unwrap_or_default();
    let most_used = usage.top(5, &ids);
    let recent = usage.recent(crate::usage::RECENT_CAP, &ids);
    let config = Config::load();
    let (folders, loose) = crate::folders::load(&apps);
    Loaded {
        apps,
        most_used,
        config,
        folders,
        loose,
        favs: crate::favorites::read(),
        recent,
        avatar: load_avatar(),
    }
}

impl App {
    fn close_popup(&mut self) -> Task<Message> {
        self.reset_popup_state();
        match self.popup.take() {
            Some(id) => cosmic::iced::platform_specific::shell::commands::popup::destroy_popup(id),
            None => Task::none(),
        }
    }

    /// Apply a change to the config: at once to the copy on screen, and to
    /// the file as it is on disk (off-thread), so a change made here can
    /// never undo one the Settings window saved, and an edit made before the
    /// first load cannot wipe the pins. The saved result replaces the copy on
    /// screen unless a newer edit has been made since.
    fn edit(&mut self, f: impl Fn(&mut Config) + Send + 'static) -> Task<Message> {
        f(&mut self.config);
        self.context = None;
        self.edit_gen = self.edit_gen.wrapping_add(1);
        let generation = self.edit_gen;
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || Config::update(f))
                    .await
                    .unwrap_or_else(|e| Err(e.to_string()))
            },
            move |r| match r {
                Ok(saved) => cosmic::action::app(Message::ConfigSaved(generation, Box::new(saved))),
                Err(e) => cosmic::action::app(Message::ShowError(e)),
            },
        )
    }

    /// Everything that belongs to one opening of the popup. Cleared however
    /// the popup goes away — our own close, Escape, or a click outside.
    fn reset_popup_state(&mut self) {
        self.power_open = false;
        self.context = None;
        self.edit = ui::tiles::Edit::default();
        self.letter_grid = false;
        self.mode_menu = false;
        self.right_menu = false;
        self.query.clear();
        self.selected = 0;
    }

    /// Height of the Most used block at the top of the list, if shown.
    fn most_used_height(&self) -> f32 {
        if !self.config.show_most_used {
            return 0.0;
        }
        let rows = self
            .usage_top
            .iter()
            .filter(|id| self.apps.iter().any(|a| &a.id == *id))
            .count();
        if rows == 0 {
            0.0
        } else {
            ui::HEADER_HEIGHT + rows as f32 * ui::ROW_HEIGHT
        }
    }

    /// Height of the Folders block: its label, one row per folder, and the
    /// apps of any folder that is open.
    fn folders_height(&self) -> f32 {
        let open: usize = self
            .open_folders
            .iter()
            .filter_map(|&i| self.folders.get(i))
            .map(|f| f.apps.len())
            .sum();
        ui::HEADER_HEIGHT + (self.folders.len() + open) as f32 * ui::ROW_HEIGHT
    }

    fn scroll_list(&self, y: f32) -> Task<Message> {
        cosmic::iced::widget::scrollable::scroll_to(
            self.list_id.clone(),
            cosmic::iced::widget::scrollable::AbsoluteOffset {
                x: None,
                y: Some(y),
            },
        )
    }

    /// Write the dock's favourites off-thread; the menu shows the new list
    /// once the write has landed, so it never shows what the dock does not.
    fn save_favourites(&mut self, list: Vec<String>) -> Task<Message> {
        self.context = None;
        Task::perform(
            async move {
                let to_write = list.clone();
                tokio::task::spawn_blocking(move || crate::favorites::write(&to_write))
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|r| r)
                    .map(|()| list)
            },
            |r| match r {
                Ok(list) => cosmic::action::app(Message::FavouritesSaved(list)),
                Err(e) => cosmic::action::app(Message::ShowError(e)),
            },
        )
    }

    fn report(&mut self, result: Result<(), String>) {
        if let Err(e) = result {
            tracing::warn!("{e}");
            self.error = Some(e);
        }
    }

    fn spacing(&self) -> Spacing {
        Spacing::from_theme(self.core.system_theme())
    }
}

impl Application for App {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    type Message = Message;

    const APP_ID: &'static str = "io.github.jjnuthuagen.StartMenu";

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, _flags: ()) -> (Self, Task<Message>) {
        (
            Self {
                core,
                popup: None,
                config: Config::default(),
                apps: Vec::new(),
                usage_top: Vec::new(),
                power_open: false,
                error: None,
                query: String::new(),
                selected: 0,
                search_id: cosmic::widget::Id::new("start-menu-search"),
                pointer: Point::ORIGIN,
                context: None,
                edit: ui::tiles::Edit::default(),
                letter_grid: false,
                list_id: cosmic::widget::Id::new("start-menu-list"),
                folders: Vec::new(),
                loose: Vec::new(),
                open_folders: std::collections::HashSet::new(),
                mode_menu: false,
                edit_gen: 0,
                favs: Vec::new(),
                recent: Vec::new(),
                right_menu: false,
                avatar: Avatar::default(),
            },
            Task::none(),
        )
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }

    fn on_close_requested(&self, id: Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::TogglePopup => {
                if self.popup.is_some() {
                    return self.close_popup();
                }
                self.power_open = false;
                self.error = None;
                self.query.clear();
                self.selected = 0;
                self.context = None;
                let id = window::Id::unique();
                self.popup = Some(id);
                let mut settings = self.core.applet.get_popup_settings(
                    self.core.main_window_id().unwrap_or(id),
                    id,
                    None,
                    None,
                    None,
                );
                settings.positioner.size_limits = Limits::NONE
                    .min_width(POPUP_WIDTH)
                    .max_width(POPUP_WIDTH)
                    .min_height(POPUP_HEIGHT)
                    .max_height(POPUP_HEIGHT);
                let popup =
                    cosmic::iced::platform_specific::shell::commands::popup::get_popup(settings);

                // Re-read apps, history and config on every open, off-thread:
                // an app installed a minute ago shows up, and edits made in the
                // Settings window apply, without a file watcher.
                let load = Task::perform(
                    async {
                        tokio::task::spawn_blocking(load)
                            .await
                            .map_err(|e| e.to_string())
                    },
                    |r| match r {
                        Ok(l) => cosmic::action::app(Message::Loaded(Box::new(l))),
                        Err(e) => cosmic::action::app(Message::ShowError(e)),
                    },
                );

                // Focused straight away so typing searches, as in Windows.
                let focus = text_input::focus(self.search_id.clone());

                // libcosmic only blurs surfaces it tracks in `surface_views`,
                // and a popup made with `get_popup` is not one of them, so the
                // theme's frosted styling would give a translucent popup with
                // nothing blurred behind it. Ask for the blur ourselves, and
                // through the Wayland command rather than `window::enable_blur`:
                // the popup's surface does not exist yet in this batch, and
                // only the Wayland path parks the request until it does.
                // (Same fix as cosmic-control-center-applet.)
                if self.core.frosted(self.core.system_theme().cosmic()) {
                    let blur = cosmic::iced::platform_specific::shell::commands::blur::blur(
                        id,
                        Some(vec![cosmic::iced::Rectangle {
                            x: 0.0,
                            y: 0.0,
                            width: f32::MAX,
                            height: f32::MAX,
                        }]),
                    );
                    Task::batch([popup, blur.discard(), load, focus])
                } else {
                    Task::batch([popup, load, focus])
                }
            }
            Message::PopupClosed(id) => {
                if self.popup == Some(id) {
                    self.popup = None;
                    self.reset_popup_state();
                }
                Task::none()
            }
            Message::Loaded(loaded) => {
                let Loaded {
                    apps,
                    most_used,
                    config,
                    folders,
                    loose,
                    favs,
                    recent,
                    avatar,
                } = *loaded;
                self.favs = favs;
                self.recent = recent;
                self.avatar = avatar;
                self.folders = folders;
                self.loose = loose;
                self.apps = apps;
                self.usage_top = most_used;
                self.config = config;
                Task::none()
            }
            Message::Launch(i) => {
                let Some(app) = self.apps.get(i).cloned() else {
                    return Task::none();
                };
                let close = self.close_popup();
                let launch = Task::perform(crate::launch::app(app), |()| cosmic::action::none());
                Task::batch([close, launch])
            }
            Message::LaunchId(id) => match self.apps.iter().position(|a| a.id == id) {
                Some(i) => self.update(Message::Launch(i)),
                None => Task::none(),
            },
            Message::LetterGrid(open) => {
                self.letter_grid = open;
                Task::none()
            }
            Message::JumpTo(letter) => {
                self.letter_grid = false;
                let (sections, prefix) = match self.config.list_mode {
                    ListMode::Folders if !self.folders.is_empty() => (
                        crate::apps::sections_of(&self.apps, &self.loose),
                        self.most_used_height() + self.folders_height(),
                    ),
                    _ => (crate::apps::sections(&self.apps), self.most_used_height()),
                };
                self.scroll_list(
                    prefix
                        + ui::app_list::offset_of(
                            &sections,
                            0,
                            &letter,
                            ui::ROW_HEIGHT,
                            ui::HEADER_HEIGHT,
                        ),
                )
            }
            Message::JumpToCategory(key) => {
                self.letter_grid = false;
                let y = self.most_used_height()
                    + ui::app_list::offset_of(
                        &crate::apps::category_sections(&self.apps),
                        0,
                        &key,
                        ui::ROW_HEIGHT,
                        ui::HEADER_HEIGHT,
                    );
                self.scroll_list(y)
            }
            Message::ModeMenu(open) => {
                self.mode_menu = open;
                Task::none()
            }
            Message::SetListMode(mode) => {
                self.mode_menu = false;
                self.letter_grid = false;
                let save = self.edit(move |c| c.list_mode = mode);
                let top = self.scroll_list(0.0);
                Task::batch([save, top])
            }
            Message::RightMenu(open) => {
                self.right_menu = open;
                Task::none()
            }
            Message::SetRightSide(side) => {
                self.right_menu = false;
                if side != RightSide::Tiles {
                    self.edit = ui::tiles::Edit::default();
                }
                self.edit(move |c| c.right_side = side)
            }
            Message::AddFavourite(id) => {
                let list = crate::favorites::with_added(&self.favs, &id);
                self.save_favourites(list)
            }
            Message::RemoveFavourite(id) => {
                let list = crate::favorites::with_removed(&self.favs, &id);
                self.save_favourites(list)
            }
            Message::ConfigSaved(generation, saved) => {
                if generation == self.edit_gen {
                    self.config = *saved;
                }
                Task::none()
            }
            Message::FavouritesSaved(list) => {
                self.favs = list;
                Task::none()
            }
            Message::ToggleFolder(i) => {
                if !self.open_folders.remove(&i) {
                    self.open_folders.insert(i);
                }
                Task::none()
            }
            Message::PowerMenu(open) => {
                self.power_open = open;
                Task::none()
            }
            Message::Power(p) => {
                self.power_open = false;
                Task::perform(crate::session::run(p), move |r| match r {
                    Ok(()) => cosmic::action::none(),
                    Err(e) => cosmic::action::app(Message::ShowError(fl!(
                        "power-failed",
                        action = fl!(p.l10n_key()),
                        error = e
                    ))),
                })
            }
            Message::Query(q) => {
                self.query = q;
                self.selected = 0;
                Task::none()
            }
            Message::SearchKey(Key::Down) => {
                let hits = crate::search::rank(&self.apps, &self.query).len();
                self.selected = (self.selected + 1).min(hits.saturating_sub(1));
                Task::none()
            }
            Message::SearchKey(Key::Up) => {
                self.selected = self.selected.saturating_sub(1);
                Task::none()
            }
            Message::SearchKey(Key::Escape) => {
                if self.query.is_empty() {
                    self.close_popup()
                } else {
                    self.query.clear();
                    self.selected = 0;
                    text_input::focus(self.search_id.clone())
                }
            }
            Message::Submit => {
                let hits = crate::search::rank(&self.apps, &self.query);
                match hits.get(self.selected.min(hits.len().saturating_sub(1))) {
                    Some(&i) => self.update(Message::Launch(i)),
                    None => Task::none(),
                }
            }
            Message::Pointer(p) => {
                self.pointer = p;
                Task::none()
            }
            Message::OpenContext(target) => {
                self.power_open = false;
                self.context = Some(Context {
                    target,
                    at: self.pointer,
                });
                Task::none()
            }
            Message::CloseContext => {
                self.context = None;
                Task::none()
            }
            Message::Pin(id) => self.edit(move |c| c.pin(&id)),
            Message::Unpin(id) => self.edit(move |c| c.unpin(&id)),
            Message::Resize(r, size) => self.edit(move |c| c.resize(r, size)),
            Message::MoveToGroup(r, g) => self.edit(move |c| c.move_tile(r, g, usize::MAX)),
            Message::NewGroupWith(r) => self.edit(move |c| {
                let g = c.add_group(fl!("new-group-name"));
                c.move_tile(r, g, 0);
            }),
            Message::RunAction(i, k) => {
                let Some(app) = self.apps.get(i).cloned() else {
                    return Task::none();
                };
                let Some(action) = app.actions.get(k).cloned() else {
                    return Task::none();
                };
                let close = self.close_popup();
                let run =
                    Task::perform(crate::launch::action(app.id, action, app.terminal), |()| {
                        cosmic::action::none()
                    });
                Task::batch([close, run])
            }
            Message::ToggleEdit => {
                // Every change in edit mode is saved as it happens, so
                // leaving it has nothing left to write.
                self.edit = ui::tiles::Edit {
                    on: !self.edit.on,
                    picked: None,
                };
                Task::none()
            }
            Message::TileClicked(at) => match self.edit.picked {
                None => {
                    self.edit.picked = Some(at);
                    Task::none()
                }
                Some(from) if from == at => {
                    self.edit.picked = None;
                    Task::none()
                }
                Some(from) => {
                    self.edit.picked = None;
                    self.edit(move |c| c.move_tile(from, at.0, at.1))
                }
            },
            Message::DropEnd(g) => match self.edit.picked.take() {
                Some(from) => self.edit(move |c| c.move_tile(from, g, usize::MAX)),
                None => Task::none(),
            },
            Message::DropNew => match self.edit.picked.take() {
                Some(from) => self.edit(move |c| {
                    let g = c.add_group(fl!("new-group-name"));
                    c.move_tile(from, g, 0);
                }),
                None => Task::none(),
            },
            Message::AddGroup => self.edit(move |c| {
                c.add_group(fl!("new-group-name"));
            }),
            Message::RenameGroup(g, name) => self.edit(move |c| c.rename_group(g, name.clone())),
            Message::RemoveGroup(g) => {
                // Group indices shift, so a picked tile's reference would
                // point at the wrong tile.
                self.edit.picked = None;
                self.edit(move |c| c.remove_group(g))
            }
            Message::OpenSettings => {
                crate::settings::open_window();
                // A window over the popup would leave it orphaned beneath.
                self.close_popup()
            }
            Message::ShowError(e) => {
                self.error = Some(e);
                Task::none()
            }
            Message::OpenFiles => {
                let r = crate::session::open_files();
                self.report(r);
                self.close_popup()
            }
            Message::OpenSettingsApp => {
                let r = crate::session::open_settings_page(None);
                self.report(r);
                self.close_popup()
            }
            Message::OpenAccount => {
                let r = crate::session::open_settings_page(Some("users"));
                self.report(r);
                self.close_popup()
            }
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        if self.popup.is_none() {
            return Subscription::none();
        }
        cosmic::iced::event::listen_with(|event, _status, _id| match event {
            cosmic::iced::Event::Keyboard(cosmic::iced::keyboard::Event::KeyPressed {
                key: KeyCode::Named(named),
                ..
            }) => match named {
                Named::ArrowUp => Some(Message::SearchKey(Key::Up)),
                Named::ArrowDown => Some(Message::SearchKey(Key::Down)),
                Named::Escape => Some(Message::SearchKey(Key::Escape)),
                _ => None,
            },
            _ => None,
        })
    }

    fn view(&self) -> Element<'_, Message> {
        let button = self
            .core
            .applet
            .icon_button("start-here-symbolic")
            .on_press(Message::TogglePopup);
        // Right-click opens Settings, as other panel items do.
        mouse_area(button)
            .on_right_press(Message::OpenSettings)
            .into()
    }

    fn view_window(&self, _id: Id) -> Element<'_, Message> {
        let spacing = self.spacing();
        let search = text_input::search_input(fl!("search-placeholder"), &self.query)
            .id(self.search_id.clone())
            .on_input(Message::Query)
            .on_submit(|_| Message::Submit);

        // While searching, results take the list and tile columns together.
        let searching = !self.query.trim().is_empty();
        let main: Element<'_, Message> = if searching {
            let hits = crate::search::rank(&self.apps, &self.query);
            let selected = self.selected.min(hits.len().saturating_sub(1));
            column::with_children(vec![
                search.into(),
                ui::app_list::results_view(&self.apps, &hits, selected, &self.query),
            ])
            .spacing(spacing.section)
            .width(Length::Fill)
            .into()
        } else {
            row::with_children(vec![
                column::with_children(vec![
                    search.width(Length::Fixed(ui::LIST_WIDTH)).into(),
                    ui::app_list::list_bar(self.config.list_mode, self.mode_menu),
                    if self.letter_grid && self.config.list_mode == ListMode::Category {
                        let present: Vec<&'static str> = crate::apps::category_sections(&self.apps)
                            .into_iter()
                            .map(|(k, _)| k)
                            .collect();
                        ui::app_list::category_grid(&present)
                    } else if self.letter_grid {
                        let sections = match self.config.list_mode {
                            ListMode::Folders if !self.folders.is_empty() => {
                                crate::apps::sections_of(&self.apps, &self.loose)
                            }
                            _ => crate::apps::sections(&self.apps),
                        };
                        let present: Vec<char> = sections.into_iter().map(|(c, _)| c).collect();
                        ui::app_list::letter_grid(&present)
                    } else {
                        ui::app_list::view(ui::app_list::ListView {
                            apps: &self.apps,
                            most_used: &self.usage_top,
                            show_most_used: self.config.show_most_used,
                            mode: self.config.list_mode,
                            folders: &self.folders,
                            loose: &self.loose,
                            open_folders: &self.open_folders,
                            list_id: self.list_id.clone(),
                        })
                    },
                ])
                .spacing(spacing.section)
                .into(),
                ui::tiles::view(ui::tiles::RightView {
                    config: &self.config,
                    apps: &self.apps,
                    spacing,
                    edit: self.edit,
                    favs: &self.favs,
                    recent: &self.recent,
                    menu_open: self.right_menu,
                }),
            ])
            .spacing(12)
            .into()
        };
        let columns = row::with_children(vec![ui::rail::view(self.power_open, &self.avatar), main])
            .spacing(12)
            .height(Length::Fill);

        let mut body = column::with_capacity(2).push(columns);
        if let Some(e) = &self.error {
            body = body.push(text::caption(e.clone()));
        }

        let body = container(body.spacing(spacing.gap))
            .padding(spacing.section)
            .width(Length::Fixed(POPUP_WIDTH))
            .height(Length::Fixed(POPUP_HEIGHT));

        // The pointer is tracked over the same box the menu is positioned
        // in, so the menu opens exactly where the right-click was.
        let tracked = mouse_area(body).on_move(Message::Pointer);
        let mut with_menu = popover(tracked).on_close(Message::CloseContext);
        if let Some(ctx) = &self.context {
            if let Some(menu) = ui::context::view(ctx, &self.apps, &self.config, &self.favs) {
                with_menu = with_menu
                    .popup(menu)
                    .position(popover::Position::Point(ctx.at));
            }
        }
        self.core.applet.popup_container(with_menu).into()
    }
}
