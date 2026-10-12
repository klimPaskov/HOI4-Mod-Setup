import unittest
from scripts.validate_repository import load_manifest_schema

class ManifestSchemaSelectionTests(unittest.TestCase):
    def test_legacy_v3_uses_legacy_schema(self):
        self.assertEqual(load_manifest_schema({'schema_version': '3.0.0'})['title'], 'HOI4 Mod Setup Remote Manifest')

    def test_v4_uses_provider_contract_schema(self):
        self.assertIn('image_generation', load_manifest_schema({'schema_version': '4.0.0'})['properties'])

    def test_unknown_or_missing_version_is_rejected(self):
        for manifest in ({}, {'schema_version': '5.0.0'}, {'schema_version': '4.bad'}, {'schema_version': 4}):
            with self.subTest(manifest=manifest), self.assertRaises(AssertionError):
                load_manifest_schema(manifest)
