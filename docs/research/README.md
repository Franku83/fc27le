# Research Notes

| File | Content |
| --- | --- |
| [`db_metadata.md`](db_metadata.md) | WP3: metadata discovery/format, Rust parser, `players = CZUM` validation, remaining CFS dependency |
| [`player_storage.md`](player_storage.md) | WP4: record storage, count/capacity/invalid markers, field encoding, verified numeric enumeration |
| [`name_storage.md`](name_storage.md) | WP5: player-name Huffman pool, `dcplayernames`, `editedplayernames`, verified name resolution |

These notes are original summaries. Raw decompiler exports, pseudocode,
proprietary DLLs, metadata XML, game files and save data are not published.

Working rules used for this research:

* no writes, calls, hooks or injection against the game;
* every live address is read through `fc27le`'s bounded reader;
* claims carry a source and a confidence level;
* a missing mapping is documented, never guessed.
