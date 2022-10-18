# Find a sqlx query involving a certain word (e.g. a certain table)

Replace "event" as necessary:

rg --multiline 'query(_scalar)?!\([^;]*\sevent(\s|"|;)'