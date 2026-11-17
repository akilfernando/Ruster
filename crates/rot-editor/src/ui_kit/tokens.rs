//! Sizes, spacing and type from UX-STANDARDS §4. Panel code uses these names,
//! never raw numbers. The UI scale preference multiplies all of them.

/// Spacing scale: every gap and padding is one of these.
pub const SPACE_XXS: f32 = 2.0;
pub const SPACE_XS: f32 = 4.0;
pub const SPACE_S: f32 = 8.0;
pub const SPACE_M: f32 = 12.0;
pub const SPACE_L: f32 = 16.0;
pub const SPACE_XL: f32 = 24.0;

/// Buttons, fields and dropdowns.
pub const CONTROL_HEIGHT: f32 = 24.0;
/// Property rows and list or tree rows.
pub const ROW_HEIGHT: f32 = 24.0;
/// Menu and context menu items.
pub const MENU_ROW: f32 = 26.0;
/// The main toolbar.
pub const TOOLBAR_HEIGHT: f32 = 36.0;
/// A dock tab strip.
pub const PANEL_TAB: f32 = 28.0;
/// Anything clickable is at least this wide and tall.
pub const MIN_HIT: f32 = 24.0;

pub const ICON: f32 = 16.0;
pub const ICON_TOOLBAR: f32 = 20.0;

pub const RADIUS_CONTROL: u8 = 4;
pub const RADIUS_MENU: u8 = 8;
pub const RADIUS_WINDOW: u8 = 10;

/// Labels and values.
pub const TEXT_BODY: f32 = 13.0;
/// Secondary detail and the status bar.
pub const TEXT_SMALL: f32 = 12.0;
/// Tab labels and section headers, in the semibold face.
pub const TEXT_TITLE: f32 = 13.0;
/// Dialog titles, in the semibold face.
pub const TEXT_DIALOG_TITLE: f32 = 15.0;
/// Console and code.
pub const TEXT_MONO: f32 = 12.0;

/// Hierarchy and folder tree indent per level.
pub const TREE_INDENT: f32 = 14.0;
/// Keyboard focus ring width.
pub const FOCUS_RING: f32 = 2.0;
/// The property label column takes this share of the panel width.
pub const LABEL_COLUMN: f32 = 0.38;
/// Smallest width of one number field in a vector row.
pub const MIN_NUMBER_FIELD: f32 = 40.0;
