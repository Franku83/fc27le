# Screenshots

These images were captured from the running FC27LE GUI and a live EA SPORTS FC
27 Career Mode session on the tested CachyOS/KDE environment. No interface state
or statistic was fabricated.

| File | What it shows |
| --- | --- |
| `fc27le-showcase.png` | EA SPORTS FC 27 Career Mode showing João Pedro's 33-goal season, with the real FC27LE Players tab for the same player. Composed from two real captures for presentation. |
| `fc27le-players.png` | FC27LE Players tab, filtered to Lionel Messi, with the real player detail panel. |
| `fc27le-lua.png` | FC27LE Lua Scripts tab after running `scripts/lua/career_stats_summary.lua`; the LOGGER output shows the live career statistics snapshot. |

The GUI captures show the tested build hash prefix, the embedded profile and the
live player/season counts. Temporary harness paths were removed from the Lua
capture by using a clean `lua_script_dirs` configuration. No user home paths,
game installation paths or exported CSV files are included.
