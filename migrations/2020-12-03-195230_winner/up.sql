CREATE TABLE winner
(
	id INTEGER NOT NULL PRIMARY KEY,
	name TEXT NOT NULL,
	how INTEGER NOT NULL,
	'when' INTEGER NOT NULL DEFAULT (datetime('now','localtime'))
)
