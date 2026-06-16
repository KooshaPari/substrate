#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! In-memory [`ToolRegistry`] and [`SkillPort`] for named agent skills/tools.

use std::collections::HashMap;

use serde_json::Value;
use substrate_core::error::{Result, SubstrateError};
use substrate_core::skill_port::{
    validate_json_schema, SkillDescriptor, SkillHandler, SkillPort, ToolRegistry,
};

/// In-memory skill registry with schema-validated invoke.
#[derive(Default)]
pub struct InMemoryToolRegistry {
    entries: HashMap<String, (SkillDescriptor, Box<dyn SkillHandler>)>,
}

impl InMemoryToolRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }
}

impl ToolRegistry for InMemoryToolRegistry {
    fn register(
        &mut self,
        descriptor: SkillDescriptor,
        handler: Box<dyn SkillHandler>,
    ) -> Result<()> {
        if self.entries.contains_key(&descriptor.name) {
            return Err(SubstrateError::Other(format!(
                "skill already registered: {}",
                descriptor.name
            )));
        }
        if !descriptor.input_schema.is_object() || !descriptor.output_schema.is_object() {
            return Err(SubstrateError::SchemaValidation(
                "input_schema and output_schema must be JSON objects".into(),
            ));
        }
        self.entries
            .insert(descriptor.name.clone(), (descriptor, handler));
        Ok(())
    }

    fn lookup(&self, name: &str) -> Option<&SkillDescriptor> {
        self.entries.get(name).map(|(d, _)| d)
    }

    fn list(&self) -> Vec<SkillDescriptor> {
        self.entries.values().map(|(d, _)| d.clone()).collect()
    }

    fn validate_input(&self, name: &str, input: &Value) -> Result<()> {
        let descriptor = self
            .lookup(name)
            .ok_or_else(|| SubstrateError::NotFound(format!("skill not found: {name}")))?;
        validate_json_schema(input, &descriptor.input_schema)
    }
}

impl SkillPort for InMemoryToolRegistry {
    fn invoke(&self, name: &str, input: Value) -> Result<Value> {
        self.validate_input(name, &input)?;
        let handler = self
            .entries
            .get(name)
            .ok_or_else(|| SubstrateError::NotFound(format!("skill not found: {name}")))?;
        handler.1.invoke(input)
    }

    fn list_skills(&self) -> Vec<SkillDescriptor> {
        self.list()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use substrate_core::skill_port::SkillHandler;

    struct EchoHandler;

    impl SkillHandler for EchoHandler {
        fn invoke(&self, input: Value) -> Result<Value> {
            Ok(input)
        }
    }

    fn echo_descriptor() -> SkillDescriptor {
        SkillDescriptor {
            name: "echo".into(),
            description: "echo input".into(),
            input_schema: json!({
                "type": "object",
                "required": ["message"],
                "properties": {
                    "message": { "type": "string" }
                }
            }),
            output_schema: json!({ "type": "object" }),
        }
    }

    #[test]
    fn register_and_lookup() {
        let mut reg = InMemoryToolRegistry::new();
        reg.register(echo_descriptor(), Box::new(EchoHandler))
            .unwrap();
        assert!(reg.lookup("echo").is_some());
        assert_eq!(reg.lookup("echo").unwrap().description, "echo input");
    }

    #[test]
    fn list_skills() {
        let mut reg = InMemoryToolRegistry::new();
        reg.register(echo_descriptor(), Box::new(EchoHandler))
            .unwrap();
        let names: Vec<_> = reg.list().into_iter().map(|d| d.name).collect();
        assert_eq!(names, vec!["echo"]);
    }

    #[test]
    fn schema_rejects_bad_input() {
        let mut reg = InMemoryToolRegistry::new();
        reg.register(echo_descriptor(), Box::new(EchoHandler))
            .unwrap();
        let err = reg.invoke("echo", json!({ "message": 42 })).unwrap_err();
        assert!(matches!(err, SubstrateError::SchemaValidation(_)));
    }

    #[test]
    fn invoke_valid_input() {
        let mut reg = InMemoryToolRegistry::new();
        reg.register(echo_descriptor(), Box::new(EchoHandler))
            .unwrap();
        let out = reg.invoke("echo", json!({ "message": "hi" })).unwrap();
        assert_eq!(out, json!({ "message": "hi" }));
    }
}
