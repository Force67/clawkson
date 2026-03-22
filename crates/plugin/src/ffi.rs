/// Dynamic plugin loading via .so/.dylib files.
///
/// This is the secondary mechanism — the primary is compiled-in via feature flags.
/// Plugins expose a C ABI entry point: `clawkson_plugin_create() -> *mut dyn ClawksonPlugin`.
#[cfg(feature = "dynamic")]
pub mod dynamic {
    use std::path::Path;
    use std::sync::Arc;

    use libloading::{Library, Symbol};
    use tracing;

    use crate::traits::ClawksonPlugin;

    /// A dynamically loaded plugin, keeping the library handle alive.
    pub struct DynamicPlugin {
        _library: Library,
        plugin: Arc<dyn ClawksonPlugin>,
    }

    impl DynamicPlugin {
        pub fn plugin(&self) -> Arc<dyn ClawksonPlugin> {
            self.plugin.clone()
        }
    }

    /// Load a plugin from a shared library file.
    ///
    /// # Safety
    /// The shared library must export a `clawkson_plugin_create` function with the correct signature.
    pub unsafe fn load_plugin(path: &Path) -> anyhow::Result<DynamicPlugin> {
        tracing::info!(path = %path.display(), "loading dynamic plugin");

        let library = unsafe { Library::new(path)? };

        // Look up the entry point
        let create_fn: Symbol<unsafe extern "C" fn() -> *mut dyn ClawksonPlugin> =
            unsafe { library.get(b"clawkson_plugin_create")? };

        let raw = unsafe { create_fn() };
        let plugin: Arc<dyn ClawksonPlugin> = unsafe { Arc::from_raw(raw) };

        Ok(DynamicPlugin {
            _library: library,
            plugin,
        })
    }
}

/// Macro for plugin authors to export a C ABI entry point.
#[macro_export]
macro_rules! export_plugin {
    ($create:expr) => {
        #[no_mangle]
        pub extern "C" fn clawkson_plugin_create() -> *mut dyn $crate::traits::ClawksonPlugin {
            let plugin = $create;
            let boxed: Box<dyn $crate::traits::ClawksonPlugin> = Box::new(plugin);
            Box::into_raw(boxed)
        }
    };
}
