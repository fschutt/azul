# R3-APPS progress (wave 9 round 3, branch wt/r3-apps from 0b8564fba)

The 10 failing app lib tests (scripts/waves/wave9/ROUND3_FAILURES.md, "App lib tests").

## DONE
1. azkeys sample::the_sample_vault_has_the_plans_items_of_every_kind - db13c2ce8 (code: 11 notes per plan)
2. azmusic playlists::a_playlist_is_one_file_named_by_its_id_and_round_trips - 48a7cabd6 (code: object + id)
3. aznews feed::an_email_author_is_shown_by_its_name - 8a76ba7f0 (test: items had no title)
4+5. aznews store::a_broken_file_is_reported_and_the_rest_loads + what_is_written_loads_back_the_same
   - 954853f56 (code: opml::write keeps the list's order)

## NEXT
6. azreader xmltree::names_and_attributes_match_without_case_and_prefix
7. azshow text::a_text_body_survives_the_trip_through_the_shared_rich_text_model
8. azshow text::an_edit_from_the_editor_comes_back_into_the_body
9. azwriter docx::the_docx_wire_subset_becomes_blocks_and_skips_the_unknown
10. azwriter commands::the_answer_of_an_import_read_is_a_document_or_a_sentence
