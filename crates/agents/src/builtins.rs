use cli_master_core::{AgentSource, CommandSpec};

use crate::{
    AgentAdapter, AgentError, DetectionResult, LaunchContext, LaunchEnvironment,
    adapter::resolved_executable,
};

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

            fn detect(&self, environment: &LaunchEnvironment) -> DetectionResult {
                environment.detect($executable)
            }

            fn build_command(&self, context: &LaunchContext) -> Result<CommandSpec, AgentError> {
                context.validate_cwd()?;
                let executable = resolved_executable(self.detect(context.environment()))?;
                let executable = executable
                    .to_str()
                    .ok_or(AgentError::NonUtf8ExecutablePath)?;

                // No vendor flags are guessed. Each CLI starts in its normal
                // interactive mode, with no copied environment variables.
                CommandSpec::new(executable, context.cwd()).map_err(AgentError::from)
            }
        }
    };
}

built_in_adapter!(CodexAdapter, "codex", "Codex", "codex");
built_in_adapter!(ClaudeCodeAdapter, "claude", "Claude Code", "claude");
built_in_adapter!(GeminiCliAdapter, "gemini", "Gemini CLI", "gemini");
built_in_adapter!(OpenCodeAdapter, "opencode", "OpenCode", "opencode");
