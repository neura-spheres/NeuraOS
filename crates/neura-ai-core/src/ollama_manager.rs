use std::process::Command;
use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaModel {
    pub name: String,
    pub model: String,
    pub modified_at: String,
    pub size: u64,
    pub digest: String,
    pub details: Option<OllamaModelDetails>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaModelDetails {
    pub parent_model: String,
    pub format: String,
    pub family: String,
    pub families: Option<Vec<String>>,
    pub parameter_size: String,
    pub quantization_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaAvailableModel {
    pub name: String,
    pub description: String,
    pub size: String,
    pub tags: Vec<String>,
    pub recommended: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelsResponse {
    pub models: Vec<OllamaModel>,
}

pub struct OllamaManager;

impl OllamaManager {
    pub fn new() -> Self {
        Self
    }

    /// Check if Ollama is installed on the system
    pub fn is_installed() -> bool {
        Command::new("ollama")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    /// Check if Ollama service is running
    pub async fn is_running() -> bool {
        // Try to connect to the Ollama API
        match reqwest::get("http://localhost:11434/api/tags").await {
            Ok(response) => response.status().is_success(),
            Err(_) => false,
        }
    }

    /// Get list of installed models
    pub async fn get_installed_models() -> Result<Vec<String>> {
        let response = reqwest::get("http://localhost:11434/api/tags")
            .await
            .context("Failed to connect to Ollama API")?;
        
        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Ollama API returned error: {}", response.status()));
        }

        // Read response text first for better error debugging
        let text = response.text().await.context("Failed to read response text")?;
        
        let models_response: ModelsResponse = serde_json::from_str(&text)
            .map_err(|e| anyhow::anyhow!("Failed to parse models response: {}. Response: {}", e, text))?;
        
        Ok(models_response.models.into_iter()
            .map(|model| model.name)
            .collect())
    }


    /// Install a model by name
    pub async fn install_model<F>(model_name: &str, progress_callback: Option<F>) -> Result<()>
    where
        F: Fn(String) + Send + Sync + 'static,
    {
        use tokio::io::AsyncBufReadExt;

        // First check if model already exists
        if Self::model_exists(model_name).await? {
            return Ok(());
        }

        // Ensure Ollama is running
        Self::ensure_ollama_running().await?;

        // Pull the model
        let mut child = tokio::process::Command::new("ollama")
            .arg("pull")
            .arg(model_name)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .context("Failed to spawn ollama pull command")?;

        let stderr = child.stderr.take().context("Failed to capture stderr")?;
        let mut reader = tokio::io::BufReader::new(stderr).lines();

        while let Ok(Some(line)) = reader.next_line().await {
            if let Some(ref cb) = progress_callback {
                cb(line);
            }
        }

        let status = child.wait().await?;

        if !status.success() {
            anyhow::bail!("Failed to install model '{}'", model_name);
        }

        Ok(())
    }

    /// Check if a specific model exists
    pub async fn model_exists(model_name: &str) -> Result<bool> {
        let models = Self::get_installed_models().await?;
        Ok(models.iter().any(|m| m.starts_with(model_name)))
    }

    /// Ensure Ollama is installed and running
    pub async fn ensure_ollama_running() -> Result<()> {
        if !Self::is_installed() {
            return Err(anyhow::anyhow!("Ollama is not installed"));
        }

        if !Self::is_running().await {
            // Try to start Ollama service
            Self::start_ollama_service().await?;
        }

        Ok(())
    }

    /// Start Ollama service
    async fn start_ollama_service() -> Result<()> {
        // Try different methods to start Ollama
        
        // Method 1: Try to start as a service (systemd)
        #[cfg(target_os = "linux")]
        {
            let output = Command::new("systemctl")
                .args(&["start", "ollama"])
                .output();
            
            if let Ok(output) = output {
                if output.status.success() {
                    // Wait a bit for service to start
                    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                    return Ok(());
                }
            }
        }

        // Method 2: Try to start Ollama directly
        let output = Command::new("ollama")
            .arg("serve")
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                // Wait a bit for service to start
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                return Ok(());
            }
        }

        Err(anyhow::anyhow!("Failed to start Ollama service"))
    }

    /// Install Ollama on the system
    pub async fn install_ollama() -> Result<()> {
        #[cfg(target_os = "windows")]
        {
            Self::install_ollama_windows().await
        }
        
        #[cfg(target_os = "macos")]
        {
            Self::install_ollama_macos().await
        }
        
        #[cfg(target_os = "linux")]
        {
            Self::install_ollama_linux().await
        }
        
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        {
            Err(anyhow::anyhow!("Unsupported operating system"))
        }
    }

    /// Install Ollama on Windows
    #[cfg(target_os = "windows")]
    async fn install_ollama_windows() -> Result<()> {
        info!("Downloading Ollama for Windows...");
        
        // Download Ollama installer
        let installer_url = "https://github.com/ollama/ollama/releases/latest/download/OllamaSetup.exe";
        let temp_dir = std::env::temp_dir();
        let installer_path = temp_dir.join("OllamaSetup.exe");
        
        let response = reqwest::get(installer_url)
            .await
            .context("Failed to download Ollama installer")?;
        
        let mut file = tokio::fs::File::create(&installer_path)
            .await
            .context("Failed to create installer file")?;
        
        let content = response.bytes()
            .await
            .context("Failed to read installer content")?;
        
        tokio::io::AsyncWriteExt::write_all(&mut file, &content)
            .await
            .context("Failed to write installer file")?;
        
        info!("Running Ollama installer...");
        
        // Run installer silently
        let output = tokio::process::Command::new(&installer_path)
            .arg("/S")
            .output()
            .await
            .context("Failed to run Ollama installer")?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Ollama installer failed: {}", stderr);
        }
        
        info!("Ollama installed successfully on Windows");
        Ok(())
    }

    /// Install Ollama on macOS
    #[cfg(target_os = "macos")]
    async fn install_ollama_macos() -> Result<()> {
        info!("Installing Ollama on macOS...");
        
        // Download and run the macOS installer script
        let script_url = "https://ollama.com/install.sh";
        let temp_dir = std::env::temp_dir();
        let script_path = temp_dir.join("ollama_install.sh");
        
        let response = reqwest::get(script_url)
            .await
            .context("Failed to download Ollama installer script")?;
        
        let script_content = response.text()
            .await
            .context("Failed to read installer script")?;
        
        tokio::fs::write(&script_path, script_content)
            .await
            .context("Failed to write installer script")?;
        
        // Make script executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = tokio::fs::metadata(&script_path).await?.permissions();
            perms.set_mode(0o755);
            tokio::fs::set_permissions(&script_path, perms).await?;
        }
        
        info!("Running Ollama installer script...");
        
        // Run the installer script
        let output = tokio::process::Command::new("/bin/bash")
            .arg(&script_path)
            .output()
            .await
            .context("Failed to run Ollama installer script")?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Ollama installer script failed: {}", stderr);
        }
        
        info!("Ollama installed successfully on macOS");
        Ok(())
    }

    /// Install Ollama on Linux
    #[cfg(target_os = "linux")]
    async fn install_ollama_linux() -> Result<()> {
        info!("Installing Ollama on Linux...");
        
        // Download and run the official installer script
        let script_url = "https://ollama.com/install.sh";
        let temp_dir = std::env::temp_dir();
        let script_path = temp_dir.join("ollama_install.sh");
        
        let response = reqwest::get(script_url)
            .await
            .context("Failed to download Ollama installer script")?;
        
        let script_content = response.text()
            .await
            .context("Failed to read installer script")?;
        
        tokio::fs::write(&script_path, script_content)
            .await
            .context("Failed to write installer script")?;
        
        // Make script executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = tokio::fs::metadata(&script_path).await?.permissions();
            perms.set_mode(0o755);
            tokio::fs::set_permissions(&script_path, perms).await?;
        }
        
        info!("Running Ollama installer script...");
        
        // Run the installer script
        let output = tokio::process::Command::new("/bin/bash")
            .arg(&script_path)
            .output()
            .await
            .context("Failed to run Ollama installer script")?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Ollama installer script failed: {}", stderr);
        }
        
        info!("Ollama installed successfully on Linux");
        Ok(())
    }

    /// Get popular models for quick selection
    pub fn get_popular_models() -> Vec<OllamaAvailableModel> {
        vec![
            // Cloud Models (Simulated/Forwarded)
            OllamaAvailableModel {
                name: "kimi-k2.5:cloud".to_string(),
                description: "Kimi K2.5 (Cloud) - High-performance general reasoning".to_string(),
                size: "Cloud".to_string(),
                tags: vec!["cloud".to_string(), "smart".to_string(), "general".to_string()],
                recommended: true,
            },
            OllamaAvailableModel {
                name: "glm-5:cloud".to_string(),
                description: "GLM-5 (Cloud) - Next-gen bilingual model".to_string(),
                size: "Cloud".to_string(),
                tags: vec!["cloud".to_string(), "bilingual".to_string()],
                recommended: true,
            },
            OllamaAvailableModel {
                name: "ministral-3:14b-cloud".to_string(),
                description: "Ministral 3 14B (Cloud) - Efficient edge reasoning".to_string(),
                size: "Cloud".to_string(),
                tags: vec!["cloud".to_string(), "fast".to_string()],
                recommended: false,
            },
            OllamaAvailableModel {
                name: "devstral-2:123b-cloud".to_string(),
                description: "Devstral 2 123B (Cloud) - Massive code specialist".to_string(),
                size: "Cloud".to_string(),
                tags: vec!["cloud".to_string(), "coding".to_string(), "expert".to_string()],
                recommended: false,
            },
            OllamaAvailableModel {
                name: "glm-4.7:cloud".to_string(),
                description: "GLM-4.7 (Cloud) - Balanced cloud performance".to_string(),
                size: "Cloud".to_string(),
                tags: vec!["cloud".to_string(), "balanced".to_string()],
                recommended: false,
            },

            // Local Models - Llama 3.2
            OllamaAvailableModel {
                name: "llama3.2:1b".to_string(),
                description: "Meta's Llama 3.2 1B - Ultra-lightweight".to_string(),
                size: "1.3GB".to_string(),
                tags: vec!["mobile".to_string(), "fast".to_string()],
                recommended: false,
            },
            OllamaAvailableModel {
                name: "llama3.2:3b".to_string(),
                description: "Meta's Llama 3.2 3B - Balanced efficiency".to_string(),
                size: "2.0GB".to_string(),
                tags: vec!["general-purpose".to_string(), "fast".to_string()],
                recommended: true,
            },

            // Local Models - Ministral
            OllamaAvailableModel {
                name: "ministral-3:14b".to_string(),
                description: "Ministral 3 14B - High capability local model".to_string(),
                size: "9.0GB".to_string(),
                tags: vec!["local".to_string(), "smart".to_string()],
                recommended: true,
            },
            OllamaAvailableModel {
                name: "ministral-3:8b".to_string(),
                description: "Ministral 3 8B - Balanced local model".to_string(),
                size: "5.5GB".to_string(),
                tags: vec!["local".to_string(), "balanced".to_string()],
                recommended: false,
            },
            OllamaAvailableModel {
                name: "ministral-3:3b".to_string(),
                description: "Ministral 3 3B - Fast local model".to_string(),
                size: "2.2GB".to_string(),
                tags: vec!["local".to_string(), "fast".to_string()],
                recommended: false,
            },

            // Other Local Models
            OllamaAvailableModel {
                name: "llama3.1:8b".to_string(),
                description: "Meta's Llama 3.1 8B - Standard reliable model".to_string(),
                size: "4.7GB".to_string(),
                tags: vec!["general-purpose".to_string()],
                recommended: false,
            },
            OllamaAvailableModel {
                name: "mistral:7b".to_string(),
                description: "Mistral 7B - High performance open model".to_string(),
                size: "4.1GB".to_string(),
                tags: vec!["general-purpose".to_string()],
                recommended: false,
            },
            OllamaAvailableModel {
                name: "gemma2:2b".to_string(),
                description: "Google's Gemma 2 2B - Very fast".to_string(),
                size: "1.6GB".to_string(),
                tags: vec!["fast".to_string(), "google".to_string()],
                recommended: false,
            },
            OllamaAvailableModel {
                name: "gemma2:9b".to_string(),
                description: "Google's Gemma 2 9B - High quality".to_string(),
                size: "5.4GB".to_string(),
                tags: vec!["smart".to_string(), "google".to_string()],
                recommended: false,
            },

            // Gemma 3
            OllamaAvailableModel {
                name: "gemma3:1b".to_string(),
                description: "Google's Gemma 3 1B - Next-gen lightweight".to_string(),
                size: "1.1GB".to_string(),
                tags: vec!["fast".to_string(), "google".to_string(), "new".to_string()],
                recommended: false,
            },
            OllamaAvailableModel {
                name: "gemma3:4b".to_string(),
                description: "Google's Gemma 3 4B - Balanced next-gen".to_string(),
                size: "2.9GB".to_string(),
                tags: vec!["balanced".to_string(), "google".to_string(), "new".to_string()],
                recommended: true,
            },
        ]
    }

    /// Uninstall a model
    pub fn uninstall_model(model_name: &str) -> Result<()> {
        let output = Command::new("ollama")
            .arg("rm")
            .arg(model_name)
            .output()
            .context("Failed to execute ollama rm command")?;

        if !output.status.success() {
            anyhow::bail!("Failed to remove model: {}", String::from_utf8_lossy(&output.stderr));
        }

        Ok(())
    }
}