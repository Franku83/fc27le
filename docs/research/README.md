# Research Notes

| File | Content |
| --- | --- |
| [`db_interface.md`](db_interface.md) | WP1: runtime database interface, call graphs, table-manager layout, missing XML name map, WP2/WP3 recommendations |
| `rea_wp1_bundle.json` | local static analysis Evidence bundle exported after the WP1 session (349 records, 2026-11-16) |

The bundle is a read-only analysis artifact. It contains pseudocode and
inventory records from `proprietary editor DLL`; it does not contain game files, save
data or player records.

Working rules used for this research:

* no writes, calls, hooks or injection against the game;
* every live address is read through `fc27le`'s bounded reader;
* claims carry a source and a confidence level;
* a missing mapping is documented, never guessed.
