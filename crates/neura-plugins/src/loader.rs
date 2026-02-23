/// Plugin loader. Will support dynamic loading of plugins from the plugins directory.
/// Placeholder until the plugin system is fully designed.
pub struct PluginLoader;

impl PluginLoader {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PluginLoader {
    fn default() -> Self {
        Self::new()
    }
}
