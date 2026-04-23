use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Default)]
pub struct CompilerRepository {
    modules: BTreeMap<String, RepositoryEntry>,
}

#[derive(Debug)]
pub struct RepositoryEntry {
    pub path: PathBuf,
    pub source: String,
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
        let parsed = crate::source::parse_source(path, &source)
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
        if self.modules.contains_key(&parsed.name) {
            return Ok(());
        }

        let base_dir = path.parent().unwrap_or(std::path::Path::new("."));
        for child in &parsed.children {
            if matches!(child.keyword.as_str(), "include" | "import") {
                if let Some(name) = &child.argument {
                    for candidate in [base_dir.join(format!("{name}.yang")), base_dir.join(format!("{name}.yin"))] {
                        if candidate.exists() {
                            self.load_recursive(&candidate)?;
                            break;
                        }
                    }
                }
            }
        }

        self.modules.insert(
            parsed.name,
            RepositoryEntry {
                path: path.clone(),
                source,
            },
        );
        Ok(())
    }

    pub fn get(&self, module_name: &str) -> Option<&str> {
        self.modules.get(module_name).map(|value| value.source.as_str())
    }

    pub fn iter(&self) -> impl Iterator<Item = &RepositoryEntry> {
        self.modules.values()
    }
}
