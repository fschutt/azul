# R3-APPS progress (wave 9 round 3, branch wt/r3-apps from 0b8564fba)

The 10 failing app lib tests (scripts/waves/wave9/ROUND3_FAILURES.md, "App lib tests").

## DONE (all 10)
1. azkeys sample::the_sample_vault_has_the_plans_items_of_every_kind - db13c2ce8 (code: 11 notes per plan)
2. azmusic playlists::a_playlist_is_one_file_named_by_its_id_and_round_trips - 48a7cabd6 (code: object + id)
3. aznews feed::an_email_author_is_shown_by_its_name - 8a76ba7f0 (test: items had no title)
4+5. aznews store::a_broken_file_is_reported_and_the_rest_loads + what_is_written_loads_back_the_same
   - 954853f56 (code: opml::write keeps the list's order)
6. azreader xmltree::names_and_attributes_match_without_case_and_prefix - RED 70ae59659, GREEN bf8123486
   (ENGINE: layout/src/xml/mod.rs feed_xml_tokens dropped attribute prefixes)
7+8+9. azshow text x2 + azwriter docx "the item keeps its level" - RED cbe5bac56, GREEN f83a2688c
   (ENGINE: layout/src/widgets/rich_text/doc.rs normalize clamped list levels)
10. azwriter commands::the_answer_of_an_import_read_is_a_document_or_a_sentence - 4f7f48337
   (code: honour docx-parser's parseError)

## NEXT
- report scripts/R3_APPS_2026_10_05.md (then done)
