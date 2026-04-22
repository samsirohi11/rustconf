use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Default)]
pub struct CompilerRepository {
    modules: BTreeMap<String, String>,
}

impl CompilerRepository {
    pub fn load_paths(paths: &[PathBuf]) -> Result<Self, std::io::Error> {
        let mut repo = Self::default();
        for path in paths {
            let source = std::fs::read_to_string(path)?;
            let name = path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_string();
            repo.modules.insert(name, source);
        }
        Ok(repo)
    }

    pub fn get(&self, module_name: &str) -> Option<&str> {
        self.modules.get(module_name).map(|value| value.as_str())
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.modules.iter()
    }
}
