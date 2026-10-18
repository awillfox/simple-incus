//! Design-system components shared by every page.
pub mod banner;
pub mod data_table;
pub mod drawer;
pub mod icons;
pub mod meter;
pub mod modal;
pub mod page;
pub mod sidebar;
pub mod status_pill;
pub mod tabs;
pub mod toast;
pub mod topbar;

pub use banner::IncusBanner;
pub use data_table::{Column, DataTable};
pub use drawer::{provide_create_drawer, use_create_drawer, CreateDrawerState, Drawer};
pub use icons::{Icon, IconName};
pub use meter::Meter;
pub use modal::ConfirmModal;
pub use page::{EmptyState, PageHeader, Skeleton};
pub use sidebar::{provide_nav_state, NavState, Sidebar};
pub use status_pill::StatusPill;
pub use tabs::Tabs;
pub use toast::{provide_toasts, toast, ToastHost, ToastKind};
pub use topbar::Topbar;
