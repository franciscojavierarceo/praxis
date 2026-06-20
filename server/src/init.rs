// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Praxis Contributors

use std::{fs, path::Path};

use crate::{InitFamily, InitRecipe};

/// Starter YAML for `praxis init responses passthrough`.
const RESPONSES_PASSTHROUGH: &str = include_str!("../../examples/configs/ai/openai/responses/responses-routing.yaml");
/// Starter YAML for `praxis init responses full-flow`.
const RESPONSES_FULL_FLOW: &str = include_str!("../../examples/configs/ai/openai/responses/full-flow.yaml");
/// Starter YAML for `praxis init messages protocol`.
const MESSAGES_PROTOCOL: &str = include_str!("../../examples/configs/ai/anthropic/messages-protocol.yaml");
/// Starter YAML for `praxis init messages to-openai`.
const MESSAGES_TO_OPENAI: &str = include_str!("../../examples/configs/ai/anthropic/messages-to-openai.yaml");

/// Write a curated starter `praxis.yaml` for the selected family and recipe.
pub(crate) fn run_init(
    output_dir: &Path,
    family: InitFamily,
    recipe: InitRecipe,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let output_path = output_dir.join("praxis.yaml");
    if output_path.exists() {
        return Err(format!("{} already exists; refusing to overwrite it", output_path.display()).into());
    }

    let template = match (family, recipe) {
        (InitFamily::Messages, InitRecipe::Protocol) => MESSAGES_PROTOCOL,
        (InitFamily::Messages, InitRecipe::ToOpenai) => MESSAGES_TO_OPENAI,
        (InitFamily::Responses, InitRecipe::Passthrough) => RESPONSES_PASSTHROUGH,
        (InitFamily::Responses, InitRecipe::FullFlow) => RESPONSES_FULL_FLOW,
        (InitFamily::Messages | InitFamily::Responses, _) => {
            return Err(format!("unsupported recipe `{recipe:?}` for family `{family:?}`").into());
        },
    };

    fs::write(&output_path, template)?;

    Ok(format!(
        "Wrote {}.\nNext steps:\n  1. Review the starter config.\n  2. Run `praxis validate -c {0}`.\n  3. Start Praxis with `praxis -c {0}`.",
        output_path.display()
    ))
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "tests exercise straightforward filesystem helpers")]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{InitFamily, InitRecipe, run_init};

    #[test]
    fn run_init_writes_default_praxis_yaml() {
        let dir = tempdir().unwrap();

        let summary = run_init(dir.path(), InitFamily::Responses, InitRecipe::Passthrough).unwrap();
        let written = dir.path().join("praxis.yaml");

        assert!(written.exists(), "init should write praxis.yaml");
        let yaml = fs::read_to_string(&written).unwrap();
        assert!(
            yaml.contains("openai_responses_format"),
            "starter config should include responses routing filters"
        );
        assert!(
            summary.contains("praxis validate"),
            "summary should tell the user how to validate the config"
        );
    }

    #[test]
    fn run_init_refuses_to_overwrite_existing_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("praxis.yaml");
        fs::write(&path, "existing: true\n").unwrap();

        let err = run_init(dir.path(), InitFamily::Responses, InitRecipe::Passthrough).unwrap_err();
        assert!(
            err.to_string().contains("already exists"),
            "existing file error should explain overwrite refusal: {err}"
        );
    }

    #[test]
    fn run_init_full_flow_uses_full_flow_template() {
        let dir = tempdir().unwrap();

        run_init(dir.path(), InitFamily::Responses, InitRecipe::FullFlow).unwrap();
        let yaml = fs::read_to_string(dir.path().join("praxis.yaml")).unwrap();

        assert!(
            yaml.contains("openai_response_store"),
            "full-flow starter should include response storage"
        );
    }

    #[test]
    fn run_init_messages_protocol_uses_messages_protocol_template() {
        let dir = tempdir().unwrap();

        run_init(dir.path(), InitFamily::Messages, InitRecipe::Protocol).unwrap();
        let yaml = fs::read_to_string(dir.path().join("praxis.yaml")).unwrap();

        assert!(
            yaml.contains("anthropic_messages_protocol"),
            "messages protocol starter should include anthropic protocol filter"
        );
    }

    #[test]
    fn run_init_messages_to_openai_uses_transform_template() {
        let dir = tempdir().unwrap();

        run_init(dir.path(), InitFamily::Messages, InitRecipe::ToOpenai).unwrap();
        let yaml = fs::read_to_string(dir.path().join("praxis.yaml")).unwrap();

        assert!(
            yaml.contains("anthropic_to_openai"),
            "messages to-openai starter should include transform filter"
        );
        assert!(
            yaml.contains("anthropic_stream_events"),
            "messages to-openai starter should include streaming transform"
        );
    }
}
