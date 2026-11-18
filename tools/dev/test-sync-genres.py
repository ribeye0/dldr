#!/usr/bin/env python3
"""Offline checks for official genre extraction and atomic catalogue updates."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("sync_genres", Path(__file__).with_name("sync-genres.py"))
sync = importlib.util.module_from_spec(spec)
spec.loader.exec_module(sync)


class GenreVerificationTests(unittest.TestCase):
    def test_heading_identity_ignores_ranks_counts_and_html_wrappers(self):
        page = sync.Page()
        page.feed('<h1><span>「被NTR(苦主视角)」的作品一览</span></h1>'
                  '<a href="https://www.dlsite.com/maniax/works/genre/=/genre/048">'
                  '<span class="ranking-number">6</span>NTR<span>(123)</span></a>')
        self.assertEqual(page.genre_ids, {48})
        self.assertEqual(sync.label(page.heading, "zh_CN"), "被NTR(苦主视角)")
        self.assertIsNone(sync.label(page.heading, "zh_TW"))
        self.assertEqual(sync.label('3D Works works', 'en_US'), '3D Works')
        self.assertEqual(sync.label('[Animal / Pet] product list', 'en_US', search=True), 'Animal / Pet')
        self.assertIsNone(sync.label('Search results', 'en_US'))

    def test_entity_text_and_official_untranslated_labels_are_preserved(self):
        page = sync.Page()
        page.feed('<h1>School &amp; Academy works</h1>')
        self.assertEqual(sync.label(page.heading, 'en_US'), 'School & Academy')
        self.assertEqual(sync.label('「TRPGシナリオ」的作品一覽', 'zh_TW'), 'TRPGシナリオ')

    def test_complete_update_keeps_old_search_names_without_guessing_translations(self):
        old = [{'id': 48, 'zhTw': 'NTR', 'zhCn': '被寝取/NTR', 'ja': '寝取られ',
                'en': 'Cuckoldry (Netorare)', 'aliases': ['older alias']}]
        labels = {48: {'zh_TW': 'NTR', 'zh_CN': '被NTR(苦主视角)', 'ja_JP': '寝取られ',
                       'en_US': 'Cuckoldry (Netorare)'}}
        row = sync.merge(old, labels)[0]
        self.assertEqual(row['zhCn'], '被NTR(苦主视角)')
        self.assertEqual(row['aliases'], ['older alias', '被寝取/NTR'])
        del labels[48]['zh_CN']
        with self.assertRaises(ValueError):
            sync.merge(old, labels)
        with self.assertRaises(ValueError):
            sync.merge(old, {})


if __name__ == '__main__':
    unittest.main()
