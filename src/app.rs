//! The applet: a panel button and the Start menu popup it opens.

use cosmic::app::{Core, Task};
use cosmic::iced::keyboard::{key::Named, Key as KeyCode};
use cosmic::iced::window::{self, Id};
use cosmic::iced::{Length, Limits, Point, Subscription};
use cosmic::widget::{column, container, mouse_area, popover, row, text, text_input};
use cosmic::{Application, Element};

use crate::apps::App as AppEntry;
use crate::config::{Config, TileRef, TileSize};
use crate::fl;
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
}

/// Read the app index, launch history and config. Blocking file I/O, so it
/// runs on the blocking pool, never in `update`.
fn load() -> Loaded {
    let apps = crate::apps::load_all();
    let ids: std::collections::HashSet<&str> = apps.iter().map(|a| a.id.as_str()).collect();
    let most_used = crate::usage::Usage::path()
        .map(|p| crate::usage::Usage::load_from(&p).top(5, &ids))
        .unwrap_or_default();
    let config = Config::load();
    Loaded {
        apps,
        most_used,
        config,
    }
}

impl App {
    fn close_popup(&mut self) -> Task<Message> {
        self.power_open = false;
        self.context = None;
        self.edit = ui::tiles::Edit::default();
        self.letter_grid = false;
        match self.popup.take() {
            Some(id) => cosmic::iced::platform_specific::shell::commands::popup::destroy_popup(id),
            None => Task::none(),
        }
    }

    /// Apply a change to the config and save it off-thread.
    fn edit(&mut self, f: impl FnOnce(&mut Config)) -> Task<Message> {
        f(&mut self.config);
        self.context = None;
        let snapshot = self.config.clone();
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || snapshot.save())
                    .await
                    .unwrap_or_else(|e| Err(e.to_string()))
            },
            |r| match r {
                Ok(()) => cosmic::action::none(),
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
                    self.power_open = false;
                }
                Task::none()
            }
            Message::Loaded(loaded) => {
                let Loaded {
                    apps,
                    most_used,
                    config,
                } = *loaded;
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
                let most_used = if self.config.show_most_used {
                    self.usage_top
                        .iter()
                        .filter(|id| self.apps.iter().any(|a| &a.id == *id))
                        .count()
                } else {
                    0
                };
                let y = ui::app_list::offset_of(
                    &crate::apps::sections(&self.apps),
                    most_used,
                    &letter,
                    ui::ROW_HEIGHT,
                    ui::HEADER_HEIGHT,
                );
                cosmic::iced::widget::scrollable::scroll_to(
                    self.list_id.clone(),
                    cosmic::iced::widget::scrollable::AbsoluteOffset {
                        x: None,
                        y: Some(y),
                    },
                )
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
            Message::Pin(id) => self.edit(|c| c.pin(&id)),
            Message::Unpin(id) => self.edit(|c| c.unpin(&id)),
            Message::Resize(r, size) => self.edit(|c| c.resize(r, size)),
            Message::MoveToGroup(r, g) => self.edit(|c| c.move_tile(r, g, usize::MAX)),
            Message::NewGroupWith(r) => self.edit(|c| {
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
                let was_on = self.edit.on;
                self.edit = ui::tiles::Edit {
                    on: !was_on,
                    picked: None,
                };
                // Renames are kept in memory while typing and saved on Done.
                if was_on {
                    self.edit(|_| {})
                } else {
                    Task::none()
                }
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
                    self.edit(|c| c.move_tile(from, at.0, at.1))
                }
            },
            Message::DropEnd(g) => match self.edit.picked.take() {
                Some(from) => self.edit(|c| c.move_tile(from, g, usize::MAX)),
                None => Task::none(),
            },
            Message::DropNew => match self.edit.picked.take() {
                Some(from) => self.edit(|c| {
                    let g = c.add_group(fl!("new-group-name"));
                    c.move_tile(from, g, 0);
                }),
                None => Task::none(),
            },
            Message::AddGroup => self.edit(|c| {
                c.add_group(fl!("new-group-name"));
            }),
            Message::RenameGroup(g, name) => {
                self.config.rename_group(g, name);
                Task::none()
            }
            Message::RemoveGroup(g) => self.edit(|c| c.remove_group(g)),
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
                    if self.letter_grid {
                        let present: Vec<char> = crate::apps::sections(&self.apps)
                            .into_iter()
                            .map(|(c, _)| c)
                            .collect();
                        ui::app_list::letter_grid(&present)
                    } else {
                        ui::app_list::view(
                            &self.apps,
                            &self.usage_top,
                            self.config.show_most_used,
                            self.list_id.clone(),
                        )
                    },
                ])
                .spacing(spacing.section)
                .into(),
                ui::tiles::view(&self.config, &self.apps, spacing, self.edit),
            ])
            .spacing(12)
            .into()
        };
        let columns = row::with_children(vec![ui::rail::view(self.power_open), main])
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
            if let Some(menu) = ui::context::view(ctx, &self.apps, &self.config) {
                with_menu = with_menu
                    .popup(menu)
                    .position(popover::Position::Point(ctx.at));
            }
        }
        self.core.applet.popup_container(with_menu).into()
    }
}
