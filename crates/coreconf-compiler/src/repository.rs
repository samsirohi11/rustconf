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
            repo.load_recursive(path)?;
        }
        Ok(repo)
    }

    fn load_recursive(&mut self, path: &PathBuf) -> Result<(), std::io::Error> {
        let source = std::fs::read_to_string(path)?;
        let parsed = crate::parse_module(&source)
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
        if self.modules.contains_key(&parsed.name) {
            return Ok(());
        }

        let base_dir = path.parent().unwrap_or(std::path::Path::new("."));
        for child in &parsed.children {
            if child.keyword == "include" {
                if let Some(include_name) = &child.argument {
                    let include_path = base_dir.join(format!("{include_name}.yang"));
                    if include_path.exists() {
                        self.load_recursive(&include_path)?;
                    }
                }
            }
        }

        self.modules.insert(parsed.name, source);
        Ok(())
    }

    pub fn get(&self, module_name: &str) -> Option<&str> {
        self.modules.get(module_name).map(|value| value.as_str())
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.modules.iter()
    }
}
