use crate::sid::SidFile;
use coreconf_schema::{CompiledSchemaBundle, SchemaNode, YangScalarType};

#[derive(Debug, Clone)]
pub struct SchemaView {
    bundle: CompiledSchemaBundle,
    legacy_sid: Option<SidFile>,
}

impl SchemaView {
    pub fn from_bundle(bundle: CompiledSchemaBundle) -> Self {
        Self {
            bundle,
            legacy_sid: None,
        }
    }

    pub fn from_sid_file(bundle: CompiledSchemaBundle, sid_file: SidFile) -> Self {
        Self {
            bundle,
            legacy_sid: Some(sid_file),
        }
    }

    pub fn get_sid(&self, path: &str) -> Option<i64> {
        self.bundle.nodes.get(path).and_then(|node| node.sid)
    }

    pub fn get_identifier(&self, sid: i64) -> Option<&str> {
        self.bundle
            .nodes
            .values()
            .find(|node| node.sid == Some(sid))
            .map(|node| node.path.as_str())
            .or_else(|| {
                self.legacy_sid
                    .as_ref()
                    .and_then(|sid_file| sid_file.get_identifier(sid))
            })
    }

    pub fn get_type(&self, path: &str) -> Option<&YangScalarType> {
        self.bundle.nodes.get(path).and_then(|node| node.yang_type.as_ref())
    }

    pub fn get_node(&self, path: &str) -> Option<&SchemaNode> {
        self.bundle.nodes.get(path)
    }

    pub fn bundle(&self) -> &CompiledSchemaBundle {
        &self.bundle
    }
}
