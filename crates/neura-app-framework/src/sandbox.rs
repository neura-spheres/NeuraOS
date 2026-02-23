pub struct AppSandbox {
    pub app_id: String,
    pub inner: neura_security::sandbox::Sandbox,
}

impl AppSandbox {
    pub fn new(app_id: &str) -> Self {
        let mut sandbox = neura_security::sandbox::Sandbox::restrictive();
        let data_dir = neura_storage::paths::app_data_dir(app_id);
        sandbox.allow_read(data_dir.clone());
        sandbox.allow_write(data_dir);
        Self {
            app_id: app_id.to_string(),
            inner: sandbox,
        }
    }
}
