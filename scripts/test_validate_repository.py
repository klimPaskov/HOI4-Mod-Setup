import unittest
from scripts.validate_repository import load_manifest_schema

class ManifestSchemaSelectionTests(unittest.TestCase):
    def test_legacy_v3_uses_legacy_schema(self):
        self.assertEqual(load_manifest_schema({'schema_version': '3.0.0'})['title'], 'HOI4 Mod Setup Remote Manifest')

    def test_v4_uses_provider_contract_schema(self):
        self.assertIn('image_generation', load_manifest_schema({'schema_version': '4.0.0'})['properties'])

    def test_v5_uses_native_linux_contract_schema(self):
        self.assertIn('linux', str(load_manifest_schema({'schema_version': '5.0.0'})))

    def test_unknown_or_missing_version_is_rejected(self):
        for manifest in ({}, {'schema_version': '6.0.0'}, {'schema_version': '4.bad'}, {'schema_version': 4}):
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
