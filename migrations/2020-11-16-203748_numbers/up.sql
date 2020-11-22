CREATE TABLE numbers
(
    id INTEGER NOT NULL PRIMARY KEY,
    number_drawn INTEGER NOT NULL,
    draw_date INTEGER NOT NULL DEFAULT (datetime('now','localtime'))
)