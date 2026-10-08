# ADDDRIVE16 - databases as files (fork of ADDDRIVE16)

Task: azul-storage's tables-as-folders data source: `tables.rs` (the layout and the SQL as
plain functions, always compiled) and `database.rs` (`DatabaseDrive` over sqlx 0.8, feature
`sql`), with their tests. No builds here; the lead compiles.

## DONE

- RED tests: src/tests/tables.rs (pure), src/tests/database.rs (a SQLite file, feature `sql`).

## IN PROGRESS

- tables.rs and database.rs.

## NEXT

- The final report to ADDDRIVE16.
