//! The applet: a panel button and the Start menu popup it opens.

use cosmic::app::{Core, Task};
use cosmic::iced::keyboard::{key::Named, Key as KeyCode};
use cosmic::iced::window::{self, Id};
use cosmic::iced::{Length, Limits, Point, Subscription};
use cosmic::widget::{column, container, mouse_area, popover, row, text, text_input};
use cosmic::{Application, Element};

use crate::apps::App as AppEntry;
use crate::config::{Config, ListMode, RightSide, Slot, TileRef, TileSize};
use crate::fl;
use crate::folders::Folder;
use crate::keynav::{self, Zone};
use crate::session::Power;
use crate::ui::{self, Spacing};

pub const POPUP_HEIGHT: f32 = 600.0;
/// What the app list's scroll viewport is worth before the column has
/// reported its real height: the menu less the search box, the All-apps bar
/// and the card's padding. Deliberately short of the truth — guessing small
/// only scrolls to a row that was already showing, guessing large would
/// leave the highlighted row off screen.
const LIST_VIEWPORT: f32 = POPUP_HEIGHT - 140.0;
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

/// How long the menu waits after losing the keyboard before closing.
const FOCUS_GRACE: std::time::Duration = std::time::Duration::from_millis(200);

/// How long after closing itself for losing the keyboard the menu ignores a
/// toggle.
///
/// Pressing the panel button while the menu is up does two things in order:
/// the press moves the keyboard to the panel, which closes the menu, and then
/// the button asks for a toggle. Without this guard that second half would
/// reopen the menu the same click had just dismissed.
const REOPEN_GUARD: std::time::Duration = std::time::Duration::from_millis(600);

/// The blur region behind the frosted menu, shaped to stay inside its
/// rounded corners. The compositor blurs rectangles as given — a single
/// whole-surface rectangle put square blurred corners behind the frame's
/// rounded ones, poking out at the menu's bottom. A cross of three
/// rectangles leaves the four `radius`×`radius` corner squares unblurred;
/// what remains of those squares inside the arc is a few pixels under an
/// already-translucent film.
fn blur_region(width: f32, height: f32, radius: f32) -> Vec<cosmic::iced::Rectangle> {
    let r = radius.clamp(0.0, width.min(height) / 2.0);
    vec![
        // The middle band, full height.
        cosmic::iced::Rectangle {
            x: r,
            y: 0.0,
            width: (width - 2.0 * r).max(0.0),
            height,
        },
        // The side bands, inset past the corner arcs.
        cosmic::iced::Rectangle {
            x: 0.0,
            y: r,
            width: r,
            height: (height - 2.0 * r).max(0.0),
        },
        cosmic::iced::Rectangle {
            x: width - r,
            y: r,
            width: r,
            height: (height - 2.0 * r).max(0.0),
        },
    ]
}

/// Whether a toggle arriving now is the tail of the click that just closed
/// the menu.
fn is_the_closing_click(closed: Option<std::time::Instant>, now: std::time::Instant) -> bool {
    closed.is_some_and(|at| now.duration_since(at) < REOPEN_GUARD)
}

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
    /// When the menu last closed itself for losing the keyboard, so the click
    /// that did it cannot reopen it. See [`REOPEN_GUARD`].
    closed_by_focus: Option<std::time::Instant>,
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
    /// A tile field mid-edit — its name or its picture path — and the text
    /// as typed so far. One at a time, like the rename it grew from.
    renaming: Option<(TileRef, TileField, String)>,
    /// The launcher's results for the current query, beyond apps.
    found: Vec<crate::launcher::Item>,
    /// The one keyboard highlight, in one of the three zones. `None` means
    /// the search box has the keyboard, which is how the menu opens.
    nav: Option<keynav::Spot>,
    /// Where the app list is scrolled to, and how tall its viewport is, so
    /// a highlighted row can be brought into view with the smallest move.
    list_offset: f32,
    list_view: f32,
}

/// The account picture, or the initial to draw when there is none.
#[derive(Debug, Clone, Default)]
pub struct Avatar {
    pub image: Option<cosmic::widget::image::Handle>,
    pub initial: String,
}

/// Decoded pixels rather than a path: COSMIC Settings saves the account
/// picture in whatever format it got (WebP here), and iced's own loader
/// does not read them all — a path handle showed the fallback initial even
/// though Settings displayed the picture fine.
fn decode_avatar(path: &std::path::Path) -> Option<cosmic::widget::image::Handle> {
    // Sniff the format from the bytes: AccountsService icons have no file
    // extension, so the extension-based `image::open` cannot name a format.
    let decoded = image::ImageReader::open(path)
        .and_then(|r| r.with_guessed_format())
        .map_err(image::ImageError::IoError)
        .and_then(|r| r.decode());
    let decoded = match decoded {
        Ok(img) => img,
        Err(e) => {
            tracing::warn!("could not decode {}: {e}", path.display());
            return None;
        }
    };
    // Drawn at 28 logical pixels; 128 keeps it crisp on any scale factor
    // without holding a full-size photo in memory.
    let small = decoded.thumbnail(128, 128).into_rgba8();
    let (w, h) = small.dimensions();
    Some(cosmic::widget::image::Handle::from_rgba(
        w,
        h,
        small.into_raw(),
    ))
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
    .find(|p| !user.is_empty() && p.is_file())
    .and_then(|p| decode_avatar(&p));
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

/// Which of a tile's text fields the inline input is editing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileField {
    /// The tile's own name, over the app's.
    Label,
    /// The path of the picture drawn behind the tile.
    Image,
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

/// Keys the open menu handles itself, whichever widget the press reached.
/// Enter is not one of them: it arrives either from the search input's own
/// submit or, once the keyboard has moved off the input, from the
/// uncaptured-key listener.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    /// Sideways, which only the tile grid has.
    Left,
    Right,
    Escape,
    /// Tab and Shift+Tab, which carry the highlight between zones.
    Tab,
    ShiftTab,
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
    /// Start renaming a tile (from its right-click menu).
    RenameTile(TileRef),
    /// Start typing a picture path for a tile (from its right-click menu).
    EditTileImage(TileRef),
    RenameText(String),
    RenameDone,
    /// Set or clear a tile's own fill colour.
    TileColor(TileRef, Option<String>),
    /// Take the picture off a tile.
    ClearTileImage(TileRef),
    /// Ctrl+1..9: launch the n'th pinned tile.
    TileNumber(usize),
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
    KeyPress(Key),
    /// A plain letter with the highlight in the app list: a jump to that
    /// letter's section rather than a character for the search box.
    Letter(char),
    /// The app list was scrolled, by the pointer or by us: where to, and
    /// how tall its viewport actually is.
    ListScrolled {
        offset: f32,
        view: f32,
    },
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
    /// One of the rail's default-app shortcuts.
    OpenSlot(Slot),
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
    /// A keyboard highlight is on something.
    highlight: bool,
    context: bool,
    renaming: bool,
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
    DropHighlight,
    Context,
    CancelRename,
    Menus,
    LetterGrid,
    DropPick,
    ClearSearch,
    LeaveEdit,
    ClosePopup,
}

/// Escape peels one layer at a time, topmost first, and closes the whole
/// popup only when nothing else is open — so dismissing a right-click menu
/// or a search never throws the user out of the Start menu. The keyboard
/// highlight is the top rung: one press puts the keyboard back in the search
/// box, which is where the menu started.
fn escape_target(l: Layers) -> Escape {
    if l.highlight {
        Escape::DropHighlight
    } else if l.context {
        Escape::Context
    } else if l.renaming {
        Escape::CancelRename
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

/// What a navigation key does when something modal is on top of the menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NavKeys {
    /// Move the highlight as asked.
    Move,
    /// Refused; the keyboard goes back to the open rename draft.
    HoldRename,
    /// Refused; the keyboard goes back to the search box.
    HoldSearch,
}

/// Whether a navigation key — Tab, Shift+Tab or an arrow onto the highlight
/// — moves it right now, and if not, where the keyboard belongs instead.
///
/// A right-click menu and an open rename draft are both modal: nothing in
/// either is focusable, so their keys arrive uncaptured and would otherwise
/// walk the highlight behind them — and the next Enter would launch the
/// highlighted row rather than acting on what was right-clicked.
///
/// Refusing is not enough on its own, because Tab has already done damage by
/// the time we see it: libcosmic's text_input marks itself read-only on Tab
/// and returns *without* consuming the press. Only a focus operation clears
/// that — a click cannot, since the mouse-press reset is gated on an
/// editable variant neither the search box nor the rename field is, and
/// neither field has an `on_tab` or `on_unfocus` of its own. So a refused key
/// hands the keyboard back to whichever input is open; left to do nothing it
/// froze the draft so hard that only Escape, which throws the draft away,
/// got out of it.
///
/// Escape is not one of these keys: its ladder already peels the top layer
/// first.
fn nav_keys(context: bool, renaming: bool) -> NavKeys {
    if renaming {
        NavKeys::HoldRename
    } else if context {
        NavKeys::HoldSearch
    } else {
        NavKeys::Move
    }
}

/// Shorthand for the keys that only need refusing, never a refocus: Enter on
/// the highlight and the first-letter jump, neither of which touches focus.
fn nav_keys_live(context: bool, renaming: bool) -> bool {
    nav_keys(context, renaming) == NavKeys::Move
}

impl App {
    /// Show the menu: a popup off the panel button, or in shortcut mode a
    /// layer surface the compositor gives the keyboard to.
    fn open(&mut self) -> Task<Message> {
        self.closed_by_focus = None;
        self.power_open = false;
        self.error = None;
        self.query.clear();
        self.selected = 0;
        self.context = None;
        self.nav = None;
        // The width and position depend on settings, and the full load
        // below only delivers after the popup is placed; edits are saved as
        // they are made, so the file is current.
        self.config = Config::peek();
        let id = window::Id::unique();
        self.popup = Some(id);
        self.opened = self.opened.wrapping_add(1);
        // One surface kind for both ways in. As a panel popup the menu could
        // not be closed by clicking the button again without a race, could
        // not take the keyboard unless a click handed it over, and a second
        // menu could open beside it from the shortcut. A layer surface is
        // focusable, dismissable and the only menu there is.
        let popup = crate::shortcut::surface(id, self.width(), self.config.menu_position);

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

        Task::batch([popup, load, focus])
    }

    /// Ask the compositor to blur what is behind the menu, so the card's
    /// translucent fill reads as COSMIC's frosted glass rather than a flat
    /// wash over whatever window it covers.
    ///
    /// libcosmic only blurs surfaces it tracks in `surface_views` — the main
    /// window and `surface-message` surfaces — and a layer surface asked for
    /// with `get_layer_surface` is in neither, so nothing requests it for us.
    ///
    /// Sent once the surface has the keyboard rather than in the batch that
    /// creates it: the menu *is* this process's first surface, so that batch
    /// runs before the Wayland platform is up and the request went nowhere —
    /// the card turned translucent with nothing blurred behind it.
    ///
    /// The region is shaped to the frame's corners: the menu is exactly
    /// `width()` × `POPUP_HEIGHT`, and `card_style` rounds it by the theme's
    /// medium radius, so a whole-surface rectangle showed square blurred
    /// corners outside the arcs.
    fn blur(&self) -> Task<Message> {
        let theme = self.core.system_theme();
        let Some(id) = self.popup else {
            return Task::none();
        };
        if !self.core.frosted(theme.cosmic()) {
            return Task::none();
        }
        let radius = theme.cosmic().corner_radii.radius_m[0];
        cosmic::iced::platform_specific::shell::commands::blur::blur(
            id,
            Some(blur_region(self.width(), POPUP_HEIGHT, radius)),
        )
        .discard()
    }

    fn close_popup(&mut self) -> Task<Message> {
        self.reset_popup_state();
        let Some(id) = self.popup.take() else {
            return Task::none();
        };
        let destroy =
            cosmic::iced::platform_specific::shell::commands::layer_surface::destroy_layer_surface(
                id,
            );
        match self.mode {
            // The applet lives on in the panel with nothing on screen.
            Mode::Panel => destroy,
            // A standalone menu is the whole process: it goes when the menu
            // does, after a pause for anything it launched.
            Mode::Shortcut => {
                let generation = self.opened;
                let exit = Task::perform(tokio::time::sleep(crate::shortcut::LINGER), move |()| {
                    cosmic::action::app(Message::Exit(generation))
                });
                Task::batch([destroy, exit])
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
        let save = Task::perform(
            async move {
                tokio::task::spawn_blocking(move || Config::update(f))
                    .await
                    .unwrap_or_else(|e| Err(e.to_string()))
            },
            move |r| match r {
                Ok(saved) => cosmic::action::app(Message::ConfigSaved(generation, Box::new(saved))),
                Err(e) => cosmic::action::app(Message::ShowError(e)),
            },
        );
        // The edit may have taken a tile out from under the highlight.
        Task::batch([self.renav(), save])
    }

    /// Everything that belongs to one opening of the popup. Cleared however
    /// the popup goes away — our own close, Escape, or a click outside.
    fn reset_popup_state(&mut self) {
        self.renaming = None;
        self.power_open = false;
        self.context = None;
        self.edit = ui::tiles::Edit::default();
        self.letter_grid = false;
        self.mode_menu = false;
        self.right_menu = false;
        self.query.clear();
        self.selected = 0;
        self.found.clear();
        self.nav = None;
        self.list_offset = 0.0;
    }

    /// The apps drawn in the pinned block, which the sections below it
    /// leave out.
    fn pinned_rows(&self) -> Vec<usize> {
        ui::app_list::pinned(
            &self.apps,
            &self.usage_top,
            &self.recent,
            self.config.show_most_used,
        )
    }

    /// Height of the Most used block at the top of the list, if shown.
    fn most_used_height(&self, pinned: &[usize]) -> f32 {
        if pinned.is_empty() {
            0.0
        } else {
            ui::ZONE_LABEL_HEIGHT + pinned.len() as f32 * ui::ROW_HEIGHT
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

    /// Scroll the list, and remember where to: `scroll_to` is an operation
    /// on the widget, not an event, so nothing reports back the offset we
    /// asked for and the next keyboard move would measure from the old one.
    fn scroll_list(&mut self, y: f32) -> Task<Message> {
        self.list_offset = y;
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

    // --- the keyboard highlight ------------------------------------------

    /// The middle column's state, borrowed for drawing it and for walking it
    /// with the keyboard — one description, so a highlight cannot land on a
    /// row the column is not showing.
    fn list_view(&self) -> ui::app_list::ListView<'_> {
        ui::app_list::ListView {
            apps: &self.apps,
            most_used: &self.usage_top,
            recent: &self.recent,
            show_most_used: self.config.show_most_used,
            mode: self.config.list_mode,
            folders: &self.folders,
            loose: &self.loose,
            open_folders: &self.open_folders,
            list_id: self.list_id.clone(),
        }
    }

    /// Whether the browse list is on screen at all: a search replaces it
    /// with results, and the letter grid replaces it with letters.
    fn browsing(&self) -> bool {
        self.query.trim().is_empty() && !self.letter_grid
    }

    /// Every row of the app list the keyboard can land on.
    fn list_stops(&self) -> Vec<ui::app_list::Stop> {
        if !self.browsing() {
            return Vec::new();
        }
        ui::app_list::stops(&ui::app_list::plan(&self.list_view()))
    }

    /// The tiles the keyboard can land on, and the grid cells they occupy —
    /// packed by the same packer the column draws them with.
    fn tile_stops(&self) -> (Vec<TileRef>, Vec<crate::tile_layout::Placement>) {
        if !self.query.trim().is_empty() {
            return (Vec::new(), Vec::new());
        }
        let (refs, groups) = ui::tiles::keyboard_tiles(&self.config, &self.apps);
        let cells = keynav::tile_grid(&groups, self.config.tile_cells());
        (refs, cells)
    }

    /// How many stops each zone has right now. While a search is on, none of
    /// them: the results have their own arrow keys and Enter, and Tab must
    /// not take the keyboard out of the box mid-query.
    fn counts(&self) -> keynav::Counts {
        if !self.query.trim().is_empty() {
            return keynav::Counts::default();
        }
        keynav::Counts {
            list: self.list_stops().len(),
            tiles: self.tile_stops().0.len(),
            rail: ui::rail::items(&self.config.system_panel).len(),
        }
    }

    /// Which stop of `zone` is highlighted, if the highlight is there.
    fn focus_in(&self, zone: Zone) -> Option<usize> {
        self.nav.filter(|s| s.zone == zone).map(|s| s.index)
    }

    /// Take the keyboard off the search box. iced's focus operation unfocuses
    /// every focusable it is not aiming at, so aiming it at an id no widget
    /// carries is how an input is blurred — otherwise a plain letter would
    /// still type into the box while the highlight was elsewhere.
    fn blur_search() -> Task<Message> {
        text_input::focus(cosmic::widget::Id::new("start-menu-nowhere"))
    }

    /// Put the highlight somewhere, or take it away. Gaining a highlight
    /// takes the keyboard off the search box; losing one hands it straight
    /// back, so Escape always leaves the menu ready to type into.
    fn set_nav(&mut self, spot: Option<keynav::Spot>) -> Task<Message> {
        let had = self.nav.is_some();
        self.nav = spot;
        match (had, self.nav.is_some()) {
            (false, true) => Task::batch([Self::blur_search(), self.reveal_nav()]),
            (true, false) => text_input::focus(self.search_id.clone()),
            (true, true) => self.reveal_nav(),
            (false, false) => Task::none(),
        }
    }

    /// Pull the highlight back into range after the rows or tiles under it
    /// have changed — a folder closed by the mouse, a tile unpinned, the
    /// background reload landing. Clamped rather than dropped, so a Tab
    /// pressed while the load was still in flight is not silently undone and
    /// the next arrow key still moves one visible step; a zone with nothing
    /// left in it hands the keyboard back to the search box.
    fn renav(&mut self) -> Task<Message> {
        let counts = self.counts();
        let to = self.nav.and_then(|s| keynav::clamped(s, counts));
        self.set_nav(to)
    }

    /// Scroll the app list so the highlighted row is showing.
    fn reveal_nav(&mut self) -> Task<Message> {
        let Some(spot) = self.nav.filter(|s| s.zone == Zone::List) else {
            return Task::none();
        };
        let Some(stop) = self.list_stops().get(spot.index).copied() else {
            return Task::none();
        };
        match keynav::reveal(stop.y, stop.h, self.list_offset, self.list_view) {
            Some(y) => self.scroll_list(y),
            None => Task::none(),
        }
    }

    /// Enter on the highlight: the very message a click on it would send, so
    /// there is one way to launch an app, open a folder or fire a rail
    /// button however the user got there.
    fn activate(&mut self) -> Task<Message> {
        let Some(spot) = self.nav else {
            return Task::none();
        };
        match spot.zone {
            Zone::List => match self.list_stops().get(spot.index).map(|s| s.act) {
                Some(ui::app_list::Act::App(i)) => self.update(Message::Launch(i)),
                Some(ui::app_list::Act::Folder(i)) => self.update(Message::ToggleFolder(i)),
                Some(ui::app_list::Act::Settings) => self.update(Message::OpenSettings),
                None => Task::none(),
            },
            Zone::Tiles => {
                let Some(at) = self.tile_stops().0.get(spot.index).copied() else {
                    return Task::none();
                };
                if self.edit.on {
                    return self.update(Message::TileClicked(at));
                }
                match self
                    .config
                    .groups
                    .get(at.0)
                    .and_then(|g| g.tiles.get(at.1))
                    .map(|t| t.app.clone())
                {
                    Some(id) => self.update(Message::LaunchId(id)),
                    None => Task::none(),
                }
            }
            Zone::Rail => {
                match ui::rail::items(&self.config.system_panel)
                    .get(spot.index)
                    .copied()
                {
                    Some(item) => {
                        let msg = ui::rail::message(item, self.power_open);
                        self.update(msg)
                    }
                    None => Task::none(),
                }
            }
        }
    }

    /// One key the whole menu listens for, wherever the keyboard is.
    fn on_key(&mut self, key: Key) -> Task<Message> {
        match key {
            Key::Escape => self.on_escape(),
            Key::Tab | Key::ShiftTab => {
                // A modal layer refuses the zone switch — and takes the
                // keyboard back, because the press has already left the
                // focused input read-only.
                match nav_keys(self.context.is_some(), self.renaming.is_some()) {
                    NavKeys::HoldRename => return text_input::focus(ui::tiles::rename_input_id()),
                    NavKeys::HoldSearch => return text_input::focus(self.search_id.clone()),
                    NavKeys::Move => {}
                }
                let to = keynav::tab(self.nav, key == Key::ShiftTab, self.counts());
                if to.is_none() && self.nav.is_none() {
                    // Nowhere to go — mid-search, every zone is empty. The
                    // keyboard stays in the search box, but libcosmic's input
                    // has already stopped taking text: with no `on_tab`
                    // handler it marks itself read-only and does not consume
                    // the press. Focusing it again is what clears that; left
                    // alone the box took no letters and no backspace, and
                    // every uncaptured letter after it was swallowed as a
                    // first-letter jump.
                    return text_input::focus(self.search_id.clone());
                }
                self.set_nav(to)
            }
            Key::Up | Key::Down | Key::Left | Key::Right => {
                let dir = match key {
                    Key::Up => keynav::Dir::Up,
                    Key::Down => keynav::Dir::Down,
                    Key::Left => keynav::Dir::Left,
                    _ => keynav::Dir::Right,
                };
                let live = nav_keys_live(self.context.is_some(), self.renaming.is_some());
                let Some(spot) = self.nav else {
                    // Down with nothing typed is the other way out of the
                    // search box, without reaching for Tab: it lands on the
                    // first row of the list. With a query in the box the
                    // arrows still walk the results, as they always have —
                    // including behind a right-click menu, which is only
                    // allowed to refuse the highlight, not the search.
                    if live && self.query.trim().is_empty() && dir == keynav::Dir::Down {
                        return self.set_nav(keynav::tab(None, false, self.counts()));
                    }
                    return self.search_arrow(dir);
                };
                // Only the highlight is gated. An arrow never trips the
                // read-only trap, so nothing needs refocusing here.
                if !live {
                    return Task::none();
                }
                let counts = self.counts();
                let (_, cells) = self.tile_stops();
                let to = keynav::moved(spot, dir, counts, &cells);
                self.set_nav(Some(to))
            }
        }
    }

    /// With no highlight the arrows belong to the search results, as they
    /// always have; sideways means nothing there.
    fn search_arrow(&mut self, dir: keynav::Dir) -> Task<Message> {
        match dir {
            keynav::Dir::Down => {
                let total = crate::search::rank(&self.apps, &self.query).len() + self.found.len();
                self.selected = (self.selected + 1).min(total.saturating_sub(1));
            }
            keynav::Dir::Up => self.selected = self.selected.saturating_sub(1),
            keynav::Dir::Left | keynav::Dir::Right => {}
        }
        Task::none()
    }

    fn on_escape(&mut self) -> Task<Message> {
        let layers = Layers {
            highlight: self.nav.is_some(),
            context: self.context.is_some(),
            renaming: self.renaming.is_some(),
            power: self.power_open,
            mode_menu: self.mode_menu,
            right_menu: self.right_menu,
            letter_grid: self.letter_grid,
            picked: self.edit.picked.is_some(),
            query: !self.query.is_empty(),
            editing: self.edit.on,
        };
        match escape_target(layers) {
            Escape::DropHighlight => return self.set_nav(None),
            Escape::Context => self.context = None,
            Escape::CancelRename => self.renaming = None,
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

    fn width(&self) -> f32 {
        popup_width(self.spacing(), self.config.tile_cells())
    }

    /// An installed app for a rail slot nobody configured: the desktop's
    /// default browser by xdg-settings, else the first app carrying the
    /// slot's freedesktop category.
    fn slot_fallback(&self, slot: Slot) -> Option<String> {
        if slot == Slot::Browser {
            if let Some(id) = crate::session::default_browser_id()
                .filter(|id| self.apps.iter().any(|a| &a.id == id))
            {
                return Some(id);
            }
        }
        let category = match slot {
            Slot::Browser => "WebBrowser",
            Slot::Files => "FileManager",
            Slot::Terminal => "TerminalEmulator",
            Slot::TaskManager => "Monitor",
        };
        self.apps
            .iter()
            .find(|a| a.categories.iter().any(|c| c == category))
            .map(|a| a.id.clone())
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
            closed_by_focus: None,
            // Peeked, not loaded: the panel button's icon and the shortcut
            // surface's position are needed before the first full load, and
            // peek never seeds a missing file.
            config: Config::peek(),
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
            renaming: None,
            found: Vec::new(),
            nav: None,
            list_offset: 0.0,
            list_view: LIST_VIEWPORT,
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
                // The panel button does not draw the menu itself: it asks the
                // menu process to appear, or to go away if it is already up.
                // That process owns a name on the bus, so however the menu is
                // asked for — this button or the keyboard shortcut — there is
                // only ever one of it.
                if self.mode == Mode::Panel {
                    crate::remote::spawn_menu();
                    return Task::none();
                }
                if self.popup.is_some() {
                    return self.close_popup();
                }
                if is_the_closing_click(self.closed_by_focus, std::time::Instant::now()) {
                    self.closed_by_focus = None;
                    return Task::none();
                }
                self.open()
            }
            Message::Focused => {
                self.had_focus = true;
                let focus = text_input::focus(self.search_id.clone());
                // The surface asks for the keyboard outright so it has it the
                // moment it maps; held that way the compositor never takes it
                // back, so a click on another window raised no `Unfocused` and
                // the menu stayed open. Once focus is actually here, hand the
                // keyboard back to on-demand so clicking away loses it.
                match self.popup {
                    Some(id) => {
                        Task::batch([focus, crate::shortcut::release_keyboard(id), self.blur()])
                    }
                    None => focus,
                }
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
                self.closed_by_focus = Some(std::time::Instant::now());
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
                // Every index a highlight could hold came from the old,
                // empty lists — but the reload lands a moment after the menu
                // opens, so dropping the highlight outright would undo a Tab
                // pressed in between.
                self.renav()
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
                // The grid replaces the list, so a highlight in it has
                // nothing left to sit on.
                if open {
                    return self.set_nav(None);
                }
                Task::none()
            }
            Message::JumpTo(letter) => {
                self.letter_grid = false;
                let pinned = self.pinned_rows();
                let (sections, prefix) = match self.config.list_mode {
                    ListMode::Folders if !self.folders.is_empty() => (
                        crate::apps::sections_of_excluding(&self.apps, &self.loose, &pinned),
                        self.most_used_height(&pinned) + self.folders_height(),
                    ),
                    _ => (
                        crate::apps::sections_excluding(&self.apps, &pinned),
                        self.most_used_height(&pinned),
                    ),
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
                let pinned = self.pinned_rows();
                let y = self.most_used_height(&pinned)
                    + ui::app_list::offset_of(
                        &crate::apps::category_sections_excluding(&self.apps, &pinned),
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
                // A different sort is a different list of rows.
                let drop = self.set_nav(None);
                let save = self.edit(move |c| c.list_mode = mode);
                let top = self.scroll_list(0.0);
                Task::batch([drop, save, top])
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
                // A different right column has different tiles, or none.
                let drop = self.set_nav(None);
                Task::batch([drop, self.edit(move |c| c.right_side = side)])
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
                // The config just came back from disk, where the Settings
                // window may have deleted the group the highlight was in.
                self.renav()
            }
            Message::FavouritesSaved(list) => {
                self.favs = list;
                Task::none()
            }
            Message::ToggleFolder(i) => {
                if !self.open_folders.remove(&i) {
                    self.open_folders.insert(i);
                }
                // Closing a folder takes its rows out of the list under the
                // highlight.
                self.renav()
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
                let was_searching = !self.query.trim().is_empty();
                self.query = q;
                self.selected = 0;
                // Typing is the search box's; the highlight belongs to the
                // browse layout the results have just replaced.
                self.nav = None;
                self.ask_launcher();
                // Starting or clearing a search swaps the whole layout under
                // the box: results take the list and tile columns together,
                // so the input sits at a different place in the widget tree
                // and iced builds it fresh there — keyboard focus stays with
                // the state it left behind. That is why the box went dead
                // after the first letter, and why Enter, which only the
                // focused input reports, never launched anything.
                if was_searching != !self.query.trim().is_empty() {
                    return Task::batch([
                        text_input::focus(self.search_id.clone()),
                        text_input::move_cursor_to_end(self.search_id.clone()),
                    ]);
                }
                Task::none()
            }
            Message::Launcher(reply) => match reply {
                // A late answer to a query since cleared would bring back
                // results for text no longer in the box.
                crate::launcher::Reply::Results(mut items) => {
                    if !self.query.trim().is_empty() {
                        items.retain(|i| self.config.keeps(i.section));
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
            Message::KeyPress(key) => self.on_key(key),
            Message::Letter(c) => {
                // Only once the highlight has reached the list: while the
                // search box has the keyboard a letter is still a letter.
                if self.focus_in(Zone::List).is_none()
                    || !nav_keys_live(self.context.is_some(), self.renaming.is_some())
                {
                    return Task::none();
                }
                let letter = crate::apps::letter(&c.to_string());
                let lines = ui::app_list::plan(&self.list_view());
                let Some(stop) = ui::app_list::stop_at_letter(&lines, letter) else {
                    return Task::none();
                };
                let jump = self.update(Message::JumpTo(letter));
                // The highlight goes with the list, so the next arrow key
                // carries on from the section he asked for rather than
                // scrolling straight back to where he was.
                self.nav = Some(keynav::Spot {
                    zone: Zone::List,
                    index: stop,
                });
                jump
            }
            Message::ListScrolled { offset, view } => {
                self.list_offset = offset;
                self.list_view = view;
                Task::none()
            }
            Message::Submit => {
                // Enter on the highlight does what clicking it would —
                // unless a right-click menu or a rename draft is on top of
                // it, which owns the key.
                if self.nav.is_some() {
                    if !nav_keys_live(self.context.is_some(), self.renaming.is_some()) {
                        return Task::none();
                    }
                    return self.activate();
                }
                // Nothing to launch with an empty box, whatever has focus.
                if self.query.trim().is_empty() {
                    return Task::none();
                }
                let hits = crate::search::rank(&self.apps, &self.query);
                match pick(hits.len(), self.found.len(), self.selected) {
                    Some(Pick::App(n)) => self.update(Message::Launch(hits[n])),
                    Some(Pick::Found(n)) => {
                        self.update(Message::LauncherActivate(self.found[n].id))
                    }
                    None => Task::none(),
                }
            }
            Message::RenameTile(at) => {
                self.context = None;
                let current = self
                    .config
                    .groups
                    .get(at.0)
                    .and_then(|g| g.tiles.get(at.1))
                    .map(|t| {
                        t.label.clone().unwrap_or_else(|| {
                            self.apps
                                .iter()
                                .find(|a| a.id == t.app)
                                .map(|a| a.name.clone())
                                .unwrap_or_default()
                        })
                    })
                    .unwrap_or_default();
                self.renaming = Some((at, TileField::Label, current));
                text_input::focus(ui::tiles::rename_input_id())
            }
            Message::EditTileImage(at) => {
                self.context = None;
                let current = self
                    .config
                    .groups
                    .get(at.0)
                    .and_then(|g| g.tiles.get(at.1))
                    .and_then(|t| t.image.clone())
                    .unwrap_or_default();
                self.renaming = Some((at, TileField::Image, current));
                text_input::focus(ui::tiles::rename_input_id())
            }
            Message::RenameText(t) => {
                if let Some((_, _, draft)) = &mut self.renaming {
                    *draft = t;
                }
                Task::none()
            }
            Message::RenameDone => {
                let Some((at, field, text)) = self.renaming.take() else {
                    return Task::none();
                };
                match field {
                    TileField::Label => self.edit(move |c| c.rename(at, &text)),
                    TileField::Image => self.edit(move |c| c.set_tile_image(at, &text)),
                }
            }
            Message::TileColor(at, color) => {
                self.edit(move |c| c.set_tile_color(at, color.clone()))
            }
            Message::ClearTileImage(at) => self.edit(move |c| c.set_tile_image(at, "")),
            Message::TileNumber(n) => {
                let installed: std::collections::HashSet<&str> =
                    self.apps.iter().map(|a| a.id.as_str()).collect();
                match self.config.nth_tile(&installed, n) {
                    Some(id) => {
                        let id = id.to_owned();
                        self.update(Message::LaunchId(id))
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
            Message::OpenSlot(slot) => {
                // The configured app when it is still installed; otherwise a
                // sensible stand-in, so the button always does something.
                let configured = self
                    .config
                    .system_panel
                    .app(slot)
                    .filter(|id| self.apps.iter().any(|a| &a.id == id))
                    .map(str::to_owned);
                if let Some(id) = configured {
                    return self.update(Message::LaunchId(id));
                }
                if let Some(id) = self.slot_fallback(slot) {
                    return self.update(Message::LaunchId(id));
                }
                match slot {
                    // The old Files behaviour: home in whatever xdg says.
                    Slot::Files => {
                        let r = crate::session::open_files();
                        self.report(r);
                        self.close_popup()
                    }
                    _ => {
                        self.error = Some(fl!("no-default-app"));
                        Task::none()
                    }
                }
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
        let keys = cosmic::iced::event::listen_with(|event, status, _id| match event {
            cosmic::iced::Event::Keyboard(cosmic::iced::keyboard::Event::KeyPressed {
                key: KeyCode::Named(named),
                modifiers,
                ..
            }) => match named {
                Named::ArrowUp => Some(Message::KeyPress(Key::Up)),
                Named::ArrowDown => Some(Message::KeyPress(Key::Down)),
                // Taken whatever consumed them: with the keyboard still in
                // the search box the input moves its cursor with these, and
                // we ignore them unless a highlight is up.
                Named::ArrowLeft => Some(Message::KeyPress(Key::Left)),
                Named::ArrowRight => Some(Message::KeyPress(Key::Right)),
                Named::Escape => Some(Message::KeyPress(Key::Escape)),
                // The search input does not consume Tab — without an
                // `on_tab` handler it only stops taking text — so the zone
                // switch is ours to make either way.
                Named::Tab => Some(Message::KeyPress(if modifiers.shift() {
                    Key::ShiftTab
                } else {
                    Key::Tab
                })),
                // The search box submits for itself while it holds the
                // keyboard focus; the arrow keys are picked up here instead,
                // and iced hands a key to the focused widget only — so after
                // arrowing into the list, Enter had nowhere to go and the
                // highlighted row could not be launched. Taken here only when
                // no widget consumed the press, so a launch never fires twice.
                Named::Enter if status == cosmic::iced::event::Status::Ignored => {
                    Some(Message::Submit)
                }
                _ => None,
            },
            // Ctrl+1..9 launches a pinned tile. Ctrl, because bare digits
            // belong to the search box ("0 A.D." is a real query).
            cosmic::iced::Event::Keyboard(cosmic::iced::keyboard::Event::KeyPressed {
                key: KeyCode::Character(c),
                modifiers,
                ..
            }) if modifiers.control() => c
                .chars()
                .next()
                .and_then(|d| d.to_digit(10))
                .filter(|d| (1..=9).contains(d))
                .map(|d| Message::TileNumber(d as usize)),
            // A plain letter once the keyboard has left the search box: a
            // jump to that letter's section. Only when nothing consumed the
            // press, so typing into the box is untouched.
            cosmic::iced::Event::Keyboard(cosmic::iced::keyboard::Event::KeyPressed {
                key: KeyCode::Character(c),
                modifiers,
                ..
            }) if !modifiers.control()
                && !modifiers.alt()
                && !modifiers.logo()
                && status == cosmic::iced::event::Status::Ignored =>
            {
                c.chars()
                    .next()
                    .filter(|c| c.is_alphanumeric())
                    .map(Message::Letter)
            }
            _ => None,
        });
        let focus = cosmic::iced::event::listen_with(|event, _status, _id| match event {
            cosmic::iced::Event::Window(window::Event::Focused) => Some(Message::Focused),
            cosmic::iced::Event::Window(window::Event::Unfocused) => Some(Message::Unfocused),
            // A layer surface loses the keyboard as a Wayland event, not a
            // window one: iced turns a keyboard leave into
            // `window::Event::Unfocused` for ordinary windows only. Watching
            // for the window event alone was why clicking another window
            // never closed the menu, however long you waited.
            cosmic::iced::Event::PlatformSpecific(
                cosmic::iced::event::PlatformSpecific::Wayland(
                    cosmic::iced::event::wayland::Event::Layer(layer, ..),
                ),
            ) => match layer {
                cosmic::iced::event::wayland::LayerEvent::Focused => Some(Message::Focused),
                cosmic::iced::event::wayland::LayerEvent::Unfocused => Some(Message::Unfocused),
                cosmic::iced::event::wayland::LayerEvent::Done => Some(Message::Unfocused),
            },
            _ => None,
        });
        Subscription::batch([remote, keys, focus])
    }

    fn view(&self) -> Element<'_, Message> {
        // No right-click menu: a missed left-click kept opening Settings,
        // which now sits as the last row of the app list instead.
        self.core
            .applet
            .icon_button(&self.config.panel_icon)
            .on_press(Message::TogglePopup)
            .into()
    }

    fn view_window(&self, _id: Id) -> Element<'_, Message> {
        let spacing = self.spacing();
        let search = text_input::search_input(fl!("search-placeholder"), &self.query)
            .id(self.search_id.clone())
            .style(ui::search_input_class())
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
                    ui::app_list::list_bar(
                        self.config.list_mode,
                        self.mode_menu,
                        self.config.locked,
                    ),
                    if self.letter_grid && self.config.list_mode == ListMode::Category {
                        let pinned = self.pinned_rows();
                        let present: Vec<&'static str> =
                            crate::apps::category_sections_excluding(&self.apps, &pinned)
                                .into_iter()
                                .map(|(k, _)| k)
                                .collect();
                        ui::app_list::category_grid(&present)
                    } else if self.letter_grid {
                        let pinned = self.pinned_rows();
                        let sections = match self.config.list_mode {
                            ListMode::Folders if !self.folders.is_empty() => {
                                crate::apps::sections_of_excluding(&self.apps, &self.loose, &pinned)
                            }
                            _ => crate::apps::sections_excluding(&self.apps, &pinned),
                        };
                        let present: Vec<char> = sections.into_iter().map(|(c, _)| c).collect();
                        ui::app_list::letter_grid(&present)
                    } else {
                        ui::app_list::view(self.list_view(), self.focus_in(Zone::List))
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
                    renaming: self.renaming.as_ref(),
                    focus: self
                        .focus_in(Zone::Tiles)
                        .and_then(|n| self.tile_stops().0.get(n).copied()),
                }),
            ])
            .spacing(COLUMN_GAP)
            .into()
        };
        let columns = row::with_children(vec![
            ui::rail::view(
                self.power_open,
                &self.avatar,
                &self.config.system_panel,
                self.focus_in(Zone::Rail),
            ),
            main,
        ])
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
        container(with_menu)
            .class(cosmic::theme::Container::custom(
                crate::shortcut::card_style,
            ))
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
            highlight: false,
            context: true,
            renaming: true,
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
                renaming: false,
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
    fn a_refused_navigation_key_hands_the_keyboard_back() {
        // Nothing on top: the key moves the highlight.
        assert_eq!(nav_keys(false, false), NavKeys::Move);
        // A rename draft is the one that must get the keyboard back, or Tab
        // leaves it read-only with no way in but Escape, which discards it.
        assert_eq!(nav_keys(false, true), NavKeys::HoldRename);
        assert_eq!(nav_keys(true, true), NavKeys::HoldRename);
        // A right-click menu with no draft open: back to the search box,
        // which Tab has just left read-only too.
        assert_eq!(nav_keys(true, false), NavKeys::HoldSearch);
    }

    #[test]
    fn a_menu_or_a_rename_on_top_owns_the_navigation_keys() {
        assert!(nav_keys_live(false, false));
        // A right-click menu: Enter must act on what was right-clicked, not
        // launch the row the highlight happens to be on behind it.
        assert!(!nav_keys_live(true, false));
        // A rename draft: Tab would blur the input and leave the draft
        // untypable.
        assert!(!nav_keys_live(false, true));
        assert!(!nav_keys_live(true, true));
    }

    #[test]
    fn escape_drops_the_highlight_before_anything_else() {
        // The top rung: one press puts the keyboard back in the search box,
        // whatever else is open behind it.
        assert_eq!(
            escape_target(Layers {
                highlight: true,
                context: true,
                renaming: true,
                power: true,
                letter_grid: true,
                picked: true,
                query: true,
                editing: true,
                ..layers()
            }),
            Escape::DropHighlight
        );
        // And with nothing else open it still only drops the highlight —
        // Escape never closes the menu while something is highlighted.
        assert_eq!(
            escape_target(Layers {
                highlight: true,
                ..layers()
            }),
            Escape::DropHighlight
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
    fn the_click_that_closed_the_menu_cannot_reopen_it() {
        let now = std::time::Instant::now();
        // The panel button's press closes the menu by moving the keyboard,
        // and the toggle it sends lands a moment later.
        assert!(is_the_closing_click(
            Some(now - std::time::Duration::from_millis(50)),
            now
        ));
        // A press a second later is someone asking for the menu again.
        assert!(!is_the_closing_click(
            Some(now - std::time::Duration::from_secs(2)),
            now
        ));
        // Nothing closed it: this is a plain open.
        assert!(!is_the_closing_click(None, now));
    }

    #[test]
    fn blur_stays_inside_the_rounded_corners() {
        let rects = blur_region(700.0, 600.0, 16.0);
        let covers = |x: f32, y: f32| {
            rects
                .iter()
                .any(|r| x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height)
        };
        // The corner squares are left alone…
        for (x, y) in [(1.0, 1.0), (699.0, 1.0), (1.0, 599.0), (699.0, 599.0)] {
            assert!(!covers(x, y), "({x},{y}) should be unblurred");
        }
        // …while the edges' midpoints and the centre are blurred.
        for (x, y) in [
            (350.0, 1.0),
            (350.0, 599.0),
            (1.0, 300.0),
            (699.0, 300.0),
            (350.0, 300.0),
        ] {
            assert!(covers(x, y), "({x},{y}) should be blurred");
        }
        // A radius bigger than the menu cannot produce negative sizes.
        for r in blur_region(20.0, 10.0, 50.0) {
            assert!(r.width >= 0.0 && r.height >= 0.0);
        }
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
    fn escape_cancels_a_rename_before_anything_below_it() {
        assert_eq!(
            escape_target(Layers {
                renaming: true,
                query: true,
                editing: true,
                ..layers()
            }),
            Escape::CancelRename
        );
        assert_eq!(
            escape_target(Layers {
                context: true,
                renaming: true,
                ..layers()
            }),
            Escape::Context
        );
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
