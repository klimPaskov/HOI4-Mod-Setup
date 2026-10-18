import unittest
from scripts.validate_repository import load_manifest_schema

class ManifestSchemaSelectionTests(unittest.TestCase):
    def test_legacy_v3_uses_legacy_schema(self):
        self.assertEqual(load_manifest_schema({'schema_version': '3.0.0'})['title'], 'HOI4 Mod Setup Remote Manifest')

    def test_v4_uses_provider_contract_schema(self):
        self.assertIn('image_generation', load_manifest_schema({'schema_version': '4.0.0'})['properties'])

    def test_v5_uses_native_linux_contract_schema(self):
        self.assertIn('linux', str(load_manifest_schema({'schema_version': '5.0.0'})))

    def test_v6_uses_per_client_mcp_contract_schema(self):
        schema = load_manifest_schema({'schema_version': '6.0.0'})
        self.assertEqual(schema['title'], 'HOI4 Mod Setup Remote Manifest v6')
        environment = schema['$defs']['codingEnvironment']
        self.assertIn('mcp_support', environment['required'])
        self.assertIn('shared_coding_environments', schema['$defs']['component']['properties'])

    def test_unknown_or_missing_version_is_rejected(self):
        for manifest in ({}, {'schema_version': '7.0.0'}, {'schema_version': '4.bad'}, {'schema_version': 4}):
            with self.subTest(manifest=manifest), self.assertRaises(AssertionError):
                load_manifest_schema(manifest)


class ProjectStateProviderSchemaTests(unittest.TestCase):
    def setUp(self):
        import json
        from pathlib import Path
        from jsonschema import Draft202012Validator
        self.root = Path(__file__).resolve().parents[1]
        self.validator = Draft202012Validator(json.loads((self.root / 'docs/schemas/project-state.schema.json').read_text(encoding='utf-8')))
        self.state = json.loads((self.root / 'docs/examples/project-state.example.json').read_text(encoding='utf-8'))
        self.selection = {'primary_provider': 'openai_api', 'additional_providers': [], 'route_settings': {}, 'credential_references': {'openai_api.api_key': 'credential://image_generation_api_key/openai_api'}}

    def test_optional_image_provider_state_accepts_only_vault_references(self):
        self.state['image_generation'] = self.selection
        self.validator.validate(self.state)

    def test_legacy_state_without_image_provider_remains_valid(self):
        self.validator.validate(self.state)

    def test_plaintext_key_and_unknown_provider_fields_are_rejected(self):
        from jsonschema import ValidationError
        self.state['image_generation'] = self.selection
        self.selection['credential_references']['openai_api.api_key'] = 'synthetic-plaintext-value'
        with self.assertRaises(ValidationError):
            self.validator.validate(self.state)
        self.selection['credential_references'].clear()
        self.selection['api_key'] = 'synthetic-plaintext-value'
        with self.assertRaises(ValidationError):
            self.validator.validate(self.state)


class DeterministicSetupSchemaTests(unittest.TestCase):
    """The deterministic-only route records provider "none" with no analysis."""

    def setUp(self):
        import json
        from pathlib import Path
        from jsonschema import Draft202012Validator
        root = Path(__file__).resolve().parents[1]
        load = lambda name: json.loads((root / name).read_text(encoding='utf-8'))
        self.state_validator = Draft202012Validator(load('docs/schemas/project-state.schema.json'))
        self.report_validator = Draft202012Validator(load('docs/schemas/readiness-report.schema.json'))
        self.state = load('docs/examples/project-state.example.json')
        self.report = load('docs/examples/readiness-report.example.json')
        self.state['ai'] = {'provider': 'none', 'model': 'deterministic', 'reasoning_effort': 'none', 'optimization_profile': 'Deterministic setup'}
        self.state['codex'] = {'integration': 'none', 'auth_mode': 'none', 'auth_status': 'not_required', 'analysis_required': False, 'analysis_status': 'not_run', 'last_error': None, 'account_values_persisted': False}
        self.report['codex'].update({'provider': 'none', 'model': 'deterministic', 'integration': 'none', 'auth_mode': 'none', 'authenticated_during_setup': False, 'analysis_status': 'not_run', 'confirmed_field_count': 0})

    def test_deterministic_records_are_valid(self):
        self.state_validator.validate(self.state)
        self.report_validator.validate(self.report)

    def test_a_deterministic_state_cannot_claim_an_analysis_or_a_model(self):
        from jsonschema import ValidationError
        for section, key, value in [('codex', 'analysis_required', True), ('codex', 'analysis_status', 'confirmed'), ('codex', 'integration', 'codex_app_server'), ('ai', 'model', 'gpt-5.6-luna'), ('ai', 'reasoning_effort', 'xhigh')]:
            state = {**self.state, section: {**self.state[section], key: value}}
            with self.assertRaises(ValidationError, msg=f'{section}.{key}'):
                self.state_validator.validate(state)

    def test_a_provider_state_cannot_skip_its_analysis(self):
        from jsonschema import ValidationError
        state = {**self.state, 'ai': {'provider': 'codex', 'model': 'gpt-5.6-luna', 'reasoning_effort': 'xhigh', 'optimization_profile': 'Codex setup analysis'}}
        with self.assertRaises(ValidationError):
            self.state_validator.validate(state)
        report = {**self.report, 'codex': {**self.report['codex'], 'provider': 'codex'}}
        with self.assertRaises(ValidationError):
            self.report_validator.validate(report)

    def test_a_deterministic_report_cannot_claim_authentication(self):
        from jsonschema import ValidationError
        report = {**self.report, 'codex': {**self.report['codex'], 'authenticated_during_setup': True}}
        with self.assertRaises(ValidationError):
            self.report_validator.validate(report)
