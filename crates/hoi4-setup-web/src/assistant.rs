//! One setup-assistant turn in the browser: the core builds the provider
//! request without the API key and validates the reply with the desktop's
//! rules.

use crate::{js, to_json};
use hoi4_setup_core::analysis::{
    analysis_input_sha256, analysis_prompt_for_provider, correctable_output_error,
    corrective_analysis_prompt, extract_structured_output, provider_request,
    validate_analysis_output, CodexAnalysis, CodexAnalysisRequest,
};
use hoi4_setup_core::AppError;
use serde::Deserialize;
use serde_json::{json, Value};
use wasm_bindgen::prelude::*;

/// A reviewed hosted route chosen in the browser.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AssistantRoute {
    provider: String,
    protocol: String,
    endpoint: String,
    model: String,
    reasoning_effort: String,
    supports_effort: bool,
    optimization_profile: String,
}

/// One assistant turn: the input hash and the request body and headers for
/// the provider, without the API key. `correction` is the validator's reason
/// from a rejected first reply.
#[wasm_bindgen(js_name = analysisTurn)]
pub fn analysis_turn(
    request_json: &str,
    route_json: &str,
    correction: Option<String>,
) -> Result<String, JsError> {
    assistant_turn(request_json, route_json, correction.as_deref()).map_err(js)
}

pub(crate) fn assistant_turn(
    request_json: &str,
    route_json: &str,
    correction: Option<&str>,
) -> Result<String, AppError> {
    let request: CodexAnalysisRequest = serde_json::from_str(request_json)?;
    let route: AssistantRoute = serde_json::from_str(route_json)?;
    let input_sha256 = analysis_input_sha256(&request)?;
    let mut prompt =
        analysis_prompt_for_provider(&request, &input_sha256, &route.optimization_profile)?;
    if let Some(reason) = correction {
        prompt = format!(
            "{prompt}\n\n{}",
            corrective_analysis_prompt(reason, &input_sha256)
        );
    }
    let provider = provider_request(
        &route.provider,
        &route.protocol,
        &route.endpoint,
        &route.model,
        &route.reasoning_effort,
        &prompt,
        route.supports_effort,
    )?;
    Ok(serde_json::to_string(&json!({
        "inputSha256": input_sha256,
        "headers": provider.headers,
        "body": provider.body,
    }))?)
}

/// The magic wand's request for a note on the chosen route, without the API
/// key: `{ "headers": [...], "body": {...} }`.
#[wasm_bindgen(js_name = descriptionTurn)]
pub fn description_turn(route_json: &str, note: &str) -> Result<String, JsError> {
    draft_turn(route_json, note).map_err(js)
}

pub(crate) fn draft_turn(route_json: &str, note: &str) -> Result<String, AppError> {
    let route: AssistantRoute = serde_json::from_str(route_json)?;
    let request = hoi4_setup_core::description::description_request(
        &route.provider,
        &route.protocol,
        &route.endpoint,
        &route.model,
        &route.reasoning_effort,
        note,
        route.supports_effort,
    )?;
    Ok(serde_json::to_string(&json!({
        "headers": request.headers,
        "body": request.body,
    }))?)
}

/// The draft in a provider's reply: `{ "description": ... }` or
/// `{ "error": message }`. A rejected draft is not retried.
#[wasm_bindgen(js_name = readDescriptionReply)]
pub fn read_description_reply(envelope_json: &str) -> Result<String, JsError> {
    to_json(&draft_reply(envelope_json))
}

pub(crate) fn draft_reply(envelope_json: &str) -> Value {
    let outcome = serde_json::from_str::<Value>(envelope_json)
        .map_err(|_| AppError::Protocol("the provider returned an unreadable response".into()))
        .and_then(|envelope| hoi4_setup_core::description::read_description_reply(&envelope));
    match outcome {
        Ok(description) => json!({ "description": description }),
        Err(error) => {
            json!({ "error": hoi4_setup_core::text::redact_secrets(&error.to_string(), &[]) })
        }
    }
}

/// Validate a provider's reply envelope. The result is `{ "analysis": ... }`,
/// `{ "retry": reason }` when one corrective turn may fix it, or
/// `{ "error": message }`. `analysis_id` is a fresh UUID from the page.
#[wasm_bindgen(js_name = readAnalysisReply)]
pub fn read_analysis_reply(
    envelope_json: &str,
    request_json: &str,
    input_sha256: &str,
    analysis_id: &str,
) -> Result<String, JsError> {
    to_json(&assistant_reply(
        envelope_json,
        request_json,
        input_sha256,
        analysis_id,
    ))
}

pub(crate) fn assistant_reply(
    envelope_json: &str,
    request_json: &str,
    input_sha256: &str,
    analysis_id: &str,
) -> Value {
    match reply_analysis(envelope_json, request_json, input_sha256, analysis_id) {
        Ok(analysis) => json!({ "analysis": analysis }),
        Err(error) => match correctable_output_error(&error) {
            Some(reason) => json!({ "retry": reason }),
            None => {
                json!({ "error": hoi4_setup_core::text::redact_secrets(&error.to_string(), &[]) })
            }
        },
    }
}

fn reply_analysis(
    envelope_json: &str,
    request_json: &str,
    input_sha256: &str,
    analysis_id: &str,
) -> Result<CodexAnalysis, AppError> {
    let request: CodexAnalysisRequest = serde_json::from_str(request_json)
        .map_err(|_| AppError::InvalidInput("the analysis request is malformed".into()))?;
    let analysis_id = analysis_id
        .parse::<uuid::Uuid>()
        .map_err(|_| AppError::InvalidInput("the analysis ID is not a UUID".into()))?;
    let envelope: Value = serde_json::from_str(envelope_json)
        .map_err(|_| AppError::Protocol("the provider returned an unreadable response".into()))?;
    let reply = extract_structured_output(&envelope)?;
    validate_analysis_output(
        reply,
        &request,
        input_sha256,
        &request.evidence,
        analysis_id,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wand_sends_only_the_note_and_reads_one_description() {
        let route = json!({
            "provider": "openrouter", "protocol": "openai_compatible",
            "endpoint": "https://openrouter.ai/api/v1/chat/completions",
            "model": "nvidia/nemotron-3.5-super-vl:free", "reasoningEffort": "high",
            "supportsEffort": false, "optimizationProfile": "OpenRouter setup analysis"
        })
        .to_string();
        let turn: Value =
            serde_json::from_str(&draft_turn(&route, "balkans 1936 mod").unwrap()).unwrap();
        assert_eq!(
            turn["body"]["response_format"]["json_schema"]["name"],
            "mod_description"
        );
        assert!(turn["body"].to_string().contains("balkans 1936 mod"));
        assert!(draft_turn(&route, "").is_err());
        let reply = json!({"choices":[{"message":{"content":"{\"description\":\"A Balkans mod.\"}"},"finish_reason":"stop"}]}).to_string();
        assert_eq!(
            draft_reply(&reply),
            json!({"description": "A Balkans mod."})
        );
        assert!(draft_reply("not json").get("error").is_some());
        let attributed = json!({"choices":[{"message":{"content":"{\"description\":\"Written by OpenRouter.\"}"},"finish_reason":"stop"}]}).to_string();
        assert!(draft_reply(&attributed).get("error").is_some());
    }

    #[test]
    fn an_assistant_turn_sends_the_schema_and_never_trusts_a_bad_reply() {
        let request = json!({
            "mode": "new_project_identity",
            "brief": "A mod about the Balkans in 1936.",
            "evidence": [],
            "constraints": {"project_id_pattern": "^[a-z][a-z0-9_]{1,63}$"}
        })
        .to_string();
        let route = json!({
            "provider": "claude",
            "protocol": "anthropic_messages",
            "endpoint": "https://api.anthropic.com/v1/messages",
            "model": "claude-haiku-5-5",
            "reasoningEffort": "medium",
            "supportsEffort": true,
            "optimizationProfile": "Claude setup analysis"
        })
        .to_string();
        let turn: Value =
            serde_json::from_str(&assistant_turn(&request, &route, None).unwrap()).unwrap();
        assert!(turn["body"]["output_config"]["format"]["schema"]["properties"].is_object());
        assert!(turn["body"].get("tools").is_none());
        assert!(!turn["headers"].to_string().contains("x-api-key"));
        let input_sha256 = turn["inputSha256"].as_str().unwrap();
        let corrected: Value = serde_json::from_str(
            &assistant_turn(&request, &route, Some("project_id is invalid")).unwrap(),
        )
        .unwrap();
        assert!(corrected["body"]["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains("rejected by deterministic validation"));

        let id = "00000000-0000-4000-8000-000000000000";
        let not_json = json!({"content": [{"type": "text", "text": "not json"}]}).to_string();
        assert!(assistant_reply(&not_json, &request, input_sha256, id)
            .get("retry")
            .is_some());
        let truncated = json!({"stop_reason": "max_tokens", "content": []}).to_string();
        assert!(assistant_reply(&truncated, &request, input_sha256, id)
            .get("error")
            .is_some());
        assert!(assistant_reply("<html>", &request, input_sha256, id)
            .get("error")
            .is_some());
    }
}
