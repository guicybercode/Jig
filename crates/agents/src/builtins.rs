use cli_master_core::AgentSource;

use crate::AgentAdapter;

macro_rules! built_in_adapter {
    ($type_name:ident, $key:literal, $display_name:literal, $executable:literal) => {
        #[doc = concat!("Built-in adapter for ", $display_name, ".")]
        #[derive(Clone, Copy, Debug, Default)]
        pub struct $type_name;

        impl AgentAdapter for $type_name {
            fn key(&self) -> &str {
                $key
            }

            fn display_name(&self) -> &str {
                $display_name
            }

            fn source(&self) -> AgentSource {
                AgentSource::BuiltIn
            }

            fn executable_name(&self) -> &str {
                $executable
            }
        }
    };
}

built_in_adapter!(CodexAdapter, "codex", "Codex", "codex");
built_in_adapter!(ClaudeCodeAdapter, "claude", "Claude Code", "claude");
built_in_adapter!(GeminiCliAdapter, "gemini", "Gemini CLI", "gemini");
built_in_adapter!(OpenCodeAdapter, "opencode", "OpenCode", "opencode");
