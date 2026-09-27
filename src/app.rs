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

pub const POPUP_HEIGHT: f32 = 600.0;
/// Between the rail, the list and the tile column.
const COLUMN_GAP: f32 = 12.0;

/// Rail + list + tile column, with padding: exactly what the columns need.
/// Summed rather than fixed, because the tile column grows with the theme's
/// density and the 2/3-across setting; a fixed width clipped the right-hand
/// tiles under Spacious. Each column scrolls inside it.
fn popup_width(spacing: Spacing, cells: u16) -> f32 {
    2.0 * f32::from(spacing.section)
        + ui::RAIL_WIDTH
        + COLUMN_GAP
        + ui::LIST_WIDTH
        + COLUMN_GAP
        + ui::tiles::column_width(spacing, cells)
}

/// The popup's size, for both the Wayland positioner and libcosmic's popup
/// frame. The frame (`popup_container`) clamps to 360 wide by default, which
/// is a Control-Center-sized popup; left alone it hides the tile column.
pub(crate) fn popup_limits(width: f32) -> Limits {
    Limits::NONE
        .min_width(width)
        .max_width(width)
        .min_height(POPUP_HEIGHT)
        .max_height(POPUP_HEIGHT)
}

/// How this process shows the menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The panel button, with the menu as its popup.
    Panel,
    /// `--toggle` from a keyboard shortcut: the menu as a layer surface of
    /// its own, open from the start, and the process ends when it closes.
    Shortcut,
}

/// Which search result the keyboard selection lands on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pick {
    /// Index into the ranked app hits.
    App(usize),
    /// Index into the launcher's results, which follow the apps.
    Found(usize),
}

/// The selection counts down through the apps, then the launcher's results;
/// past the end it sticks on the last row, as the arrow keys do.
fn pick(apps: usize, found: usize, selected: usize) -> Option<Pick> {
    let last = (apps + found).checked_sub(1)?;
    let n = selected.min(last);
    Some(if n < apps {
        Pick::App(n)
    } else {
        Pick::Found(n - apps)
    })
}

/// How long the shortcut menu waits after losing the keyboard before closing.
const FOCUS_GRACE: std::time::Duration = std::time::Duration::from_millis(200);

pub struct App {
    core: Core,
    mode: Mode,
    /// Bumped whenever a shortcut menu opens, so an exit scheduled by an
    /// earlier close can tell it is stale.
    opened: u64,
    /// Whether the shortcut menu has had the keyboard yet. The surface
    /// reports losing focus once before it first gains it, which is not a
    /// click away.
    had_focus: bool,
    /// Counts `Unfocused` events, so a `FocusGone` check can tell whether
    /// focus came back (a `Focused` in between) since it was scheduled.
    focus_losses: u64,
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
    /// The launcher's results for the current query, beyond apps.
    found: Vec<crate::launcher::Item>,
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
    /// The shortcut menu got the keyboard: now the search box can take it.
    /// Focusing it at open does nothing, the surface does not exist yet.
    Focused,
    /// The shortcut menu lost the keyboard, e.g. to a click on a window.
    Unfocused,
    /// `FOCUS_GRACE` after an `Unfocused`: close if the keyboard has not
    /// come back since. The number is the `focus_losses` count it belongs to.
    FocusGone(u64),
    /// A shortcut menu closed `LINGER` ago; exit unless it reopened since.
    Exit(u64),
    /// Something back from COSMIC's launcher service.
    Launcher(crate::launcher::Reply),
    /// Run one of the launcher's results.
    LauncherActivate(u32),
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

/// What is open on top of the popup, for deciding what Escape closes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Layers {
    context: bool,
    power: bool,
    mode_menu: bool,
    right_menu: bool,
    letter_grid: bool,
    picked: bool,
    query: bool,
    editing: bool,
}

/// What one press of Escape does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Escape {
    Context,
    Menus,
    LetterGrid,
    DropPick,
    ClearSearch,
    LeaveEdit,
    ClosePopup,
}

/// Escape peels one layer at a time, topmost first, and closes the whole
/// popup only when nothing else is open — so dismissing a right-click menu
/// or a search never throws the user out of the Start menu.
fn escape_target(l: Layers) -> Escape {
    if l.context {
        Escape::Context
    } else if l.power || l.mode_menu || l.right_menu {
        Escape::Menus
    } else if l.letter_grid {
        Escape::LetterGrid
    } else if l.picked {
        Escape::DropPick
    } else if l.query {
        Escape::ClearSearch
    } else if l.editing {
        Escape::LeaveEdit
    } else {
        Escape::ClosePopup
    }
}

impl App {
    /// Show the menu: a popup off the panel button, or in shortcut mode a
    /// layer surface the compositor gives the keyboard to.
    fn open(&mut self) -> Task<Message> {
        self.power_open = false;
        self.error = None;
        self.query.clear();
        self.selected = 0;
        self.context = None;
        // The width depends on the 2/3-across setting, which the full
        // load below only delivers after the popup is placed.
        self.config.tile_columns = Config::peek().tile_columns;
        let id = window::Id::unique();
        self.popup = Some(id);
        self.opened = self.opened.wrapping_add(1);
        let popup = match self.mode {
            Mode::Panel => {
                let mut settings = self.core.applet.get_popup_settings(
                    self.core.main_window_id().unwrap_or(id),
                    id,
                    None,
                    None,
                    None,
                );
                settings.positioner.size_limits = popup_limits(self.width());
                cosmic::iced::platform_specific::shell::commands::popup::get_popup(settings)
            }
            Mode::Shortcut => crate::shortcut::surface(id, self.width()),
        };

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
        // The shortcut menu's card is opaque instead: see `shortcut::card_style`.
        if self.mode == Mode::Panel && self.core.frosted(self.core.system_theme().cosmic()) {
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

    fn close_popup(&mut self) -> Task<Message> {
        self.reset_popup_state();
        let Some(id) = self.popup.take() else {
            return Task::none();
        };
        match self.mode {
            Mode::Panel => {
                cosmic::iced::platform_specific::shell::commands::popup::destroy_popup(id)
            }
            Mode::Shortcut => {
                let generation = self.opened;
                let exit = Task::perform(tokio::time::sleep(crate::shortcut::LINGER), move |()| {
                    cosmic::action::app(Message::Exit(generation))
                });
                Task::batch([
                    cosmic::iced::platform_specific::shell::commands::layer_surface::destroy_layer_surface(id),
                    exit,
                ])
            }
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
        self.found.clear();
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
        match self.mode {
            Mode::Panel => Spacing::from_theme(self.core.system_theme()),
            // The shortcut menu sizes its surface in `init`, before the
            // theme has loaded: the default theme's spacing made the surface
            // too narrow for Spacious gaps, squeezing the last tile column.
            // One source for both the size and the layout keeps them agreeing.
            Mode::Shortcut => Spacing::from_density(),
        }
    }

    /// Ask the launcher about the current query, or drop its old answer.
    fn ask_launcher(&mut self) {
        let q = self.query.trim();
        if q.is_empty() {
            self.found.clear();
        } else {
            crate::launcher::search(q);
        }
    }

    fn width(&self) -> f32 {
        popup_width(self.spacing(), self.config.tile_cells())
    }
}

impl Application for App {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = Mode;
    type Message = Message;

    const APP_ID: &'static str = "io.github.jjnuthuagen.StartMenu";

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, mode: Mode) -> (Self, Task<Message>) {
        let mut app = Self {
            core,
            mode,
            opened: 0,
            had_focus: false,
            focus_losses: 0,
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
            found: Vec::new(),
        };
        let open = match mode {
            Mode::Panel => Task::none(),
            Mode::Shortcut => app.open(),
        };
        (app, open)
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
                self.open()
            }
            Message::Focused => {
                self.had_focus = true;
                text_input::focus(self.search_id.clone())
            }
            Message::Unfocused => {
                if !self.had_focus {
                    return Task::none();
                }
                // Not closed at once: focus also leaves and comes straight
                // back when a keyboard device appears (a virtual keyboard, a
                // layout switch), and that is not a click away.
                self.had_focus = false;
                self.focus_losses = self.focus_losses.wrapping_add(1);
                let loss = self.focus_losses;
                Task::perform(tokio::time::sleep(FOCUS_GRACE), move |()| {
                    cosmic::action::app(Message::FocusGone(loss))
                })
            }
            Message::FocusGone(loss) => {
                if self.had_focus || loss != self.focus_losses {
                    return Task::none();
                }
                self.close_popup()
            }
            Message::Exit(generation) => {
                if self.popup.is_none() && generation == self.opened {
                    return cosmic::iced::exit();
                }
                Task::none()
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
                self.ask_launcher();
                Task::none()
            }
            Message::Launcher(reply) => match reply {
                // A late answer to a query since cleared would bring back
                // results for text no longer in the box.
                crate::launcher::Reply::Results(items) => {
                    if !self.query.trim().is_empty() {
                        self.found = items;
                    }
                    Task::none()
                }
                crate::launcher::Reply::Fill(text) => {
                    self.query = text;
                    self.selected = 0;
                    self.ask_launcher();
                    text_input::move_cursor_to_end(self.search_id.clone())
                }
                crate::launcher::Reply::Close => self.close_popup(),
            },
            Message::LauncherActivate(id) => {
                crate::launcher::activate(id);
                Task::none()
            }
            Message::SearchKey(Key::Down) => {
                let total = crate::search::rank(&self.apps, &self.query).len() + self.found.len();
                self.selected = (self.selected + 1).min(total.saturating_sub(1));
                Task::none()
            }
            Message::SearchKey(Key::Up) => {
                self.selected = self.selected.saturating_sub(1);
                Task::none()
            }
            Message::SearchKey(Key::Escape) => {
                let layers = Layers {
                    context: self.context.is_some(),
                    power: self.power_open,
                    mode_menu: self.mode_menu,
                    right_menu: self.right_menu,
                    letter_grid: self.letter_grid,
                    picked: self.edit.picked.is_some(),
                    query: !self.query.is_empty(),
                    editing: self.edit.on,
                };
                match escape_target(layers) {
                    Escape::Context => self.context = None,
                    Escape::Menus => {
                        self.power_open = false;
                        self.mode_menu = false;
                        self.right_menu = false;
                    }
                    Escape::LetterGrid => self.letter_grid = false,
                    Escape::DropPick => self.edit.picked = None,
                    Escape::ClearSearch => {
                        self.query.clear();
                        self.selected = 0;
                        self.found.clear();
                        return text_input::focus(self.search_id.clone());
                    }
                    Escape::LeaveEdit => self.edit = ui::tiles::Edit::default(),
                    Escape::ClosePopup => return self.close_popup(),
                }
                Task::none()
            }
            Message::Submit => {
                let hits = crate::search::rank(&self.apps, &self.query);
                match pick(hits.len(), self.found.len(), self.selected) {
                    Some(Pick::App(n)) => self.update(Message::Launch(hits[n])),
                    Some(Pick::Found(n)) => {
                        self.update(Message::LauncherActivate(self.found[n].id))
                    }
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
        // A second shortcut press, asking this shortcut menu to close.
        let remote = Subscription::run_with((), |()| {
            futures::stream::unfold(crate::remote::requests(), |requests| async move {
                let mut receiver = requests?;
                receiver
                    .recv()
                    .await
                    .map(|()| (Message::TogglePopup, Some(receiver)))
            })
        });
        let launcher = Subscription::run_with((), |()| {
            futures::stream::unfold(crate::launcher::receiver(), |replies| async move {
                let mut receiver = replies?;
                receiver
                    .recv()
                    .await
                    .map(|reply| (Message::Launcher(reply), Some(receiver)))
            })
        });
        let remote = Subscription::batch([remote, launcher]);
        if self.popup.is_none() {
            return remote;
        }
        let keys = cosmic::iced::event::listen_with(|event, _status, _id| match event {
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
        });
        if self.mode == Mode::Panel {
            return Subscription::batch([remote, keys]);
        }
        let focus = cosmic::iced::event::listen_with(|event, _status, _id| match event {
            cosmic::iced::Event::Window(window::Event::Focused) => Some(Message::Focused),
            cosmic::iced::Event::Window(window::Event::Unfocused) => Some(Message::Unfocused),
            _ => None,
        });
        Subscription::batch([remote, keys, focus])
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
            let selected = self
                .selected
                .min((hits.len() + self.found.len()).saturating_sub(1));
            column::with_children(vec![
                search.into(),
                ui::app_list::results_view(&self.apps, &hits, &self.found, selected, &self.query),
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
            .spacing(COLUMN_GAP)
            .into()
        };
        let columns = row::with_children(vec![ui::rail::view(self.power_open, &self.avatar), main])
            .spacing(COLUMN_GAP)
            .height(Length::Fill);

        let mut body = column::with_capacity(2).push(columns);
        if let Some(e) = &self.error {
            body = body.push(text::caption(e.clone()));
        }

        let body = container(body.spacing(spacing.gap))
            .padding(spacing.section)
            .width(Length::Fixed(self.width()))
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
        if self.mode == Mode::Shortcut {
            return container(with_menu)
                .class(cosmic::theme::Container::custom(
                    crate::shortcut::card_style,
                ))
                .into();
        }
        self.core
            .applet
            .popup_container(with_menu)
            .limits(popup_limits(self.width()))
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layers() -> Layers {
        Layers::default()
    }

    #[test]
    fn escape_closes_the_topmost_thing_first() {
        let all = Layers {
            context: true,
            power: true,
            mode_menu: true,
            right_menu: true,
            letter_grid: true,
            picked: true,
            query: true,
            editing: true,
        };
        assert_eq!(escape_target(all), Escape::Context);
        assert_eq!(
            escape_target(Layers {
                context: false,
                ..all
            }),
            Escape::Menus
        );
        assert_eq!(
            escape_target(Layers {
                power: true,
                ..layers()
            }),
            Escape::Menus
        );
        assert_eq!(
            escape_target(Layers {
                letter_grid: true,
                query: true,
                ..layers()
            }),
            Escape::LetterGrid
        );
        assert_eq!(
            escape_target(Layers {
                picked: true,
                query: true,
                ..layers()
            }),
            Escape::DropPick
        );
        assert_eq!(
            escape_target(Layers {
                query: true,
                ..layers()
            }),
            Escape::ClearSearch
        );
    }

    #[test]
    fn escape_leaves_edit_mode_after_clearing_a_search() {
        assert_eq!(
            escape_target(Layers {
                editing: true,
                ..layers()
            }),
            Escape::LeaveEdit
        );
    }

    #[test]
    fn popup_frame_is_as_wide_as_the_menu() {
        // libcosmic's popup_container clamps every applet popup to 360 wide
        // unless told otherwise, which cut the tile column off entirely.
        let max = popup_limits(700.0).max();
        assert_eq!(max.width, 700.0);
        assert_eq!(popup_limits(700.0).min().width, 700.0);
        assert!(max.height >= POPUP_HEIGHT);
    }

    #[test]
    fn popup_fits_the_tile_column_at_every_density() {
        // Spacious gaps once pushed the third tile column past a fixed
        // 680, and the popup edge clipped it narrower than the other two.
        for gap in [4, 8, 12] {
            let spacing = Spacing {
                gap,
                pad_y: gap,
                section: gap + 4,
            };
            for cells in [4, 6] {
                let used = 2.0 * f32::from(spacing.section)
                    + ui::RAIL_WIDTH
                    + ui::LIST_WIDTH
                    + 2.0 * COLUMN_GAP
                    + ui::tiles::grid_width(spacing, cells);
                assert!(popup_width(spacing, cells) >= used);
            }
        }
    }

    #[test]
    fn enter_picks_apps_first_then_launcher_results() {
        assert_eq!(pick(0, 0, 0), None);
        assert_eq!(pick(2, 3, 0), Some(Pick::App(0)));
        assert_eq!(pick(2, 3, 1), Some(Pick::App(1)));
        assert_eq!(pick(2, 3, 2), Some(Pick::Found(0)));
        assert_eq!(pick(2, 3, 4), Some(Pick::Found(2)));
        // A selection left over from a longer list clamps to the last row.
        assert_eq!(pick(2, 3, 9), Some(Pick::Found(2)));
        assert_eq!(pick(0, 1, 5), Some(Pick::Found(0)));
        assert_eq!(pick(3, 0, 5), Some(Pick::App(2)));
    }

    #[test]
    fn escape_closes_the_popup_only_when_nothing_else_is_open() {
        assert_eq!(escape_target(layers()), Escape::ClosePopup);
    }
}
