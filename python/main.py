"""
Write to a sqlite database from multiple threads, causing contention.
"""

from concurrent.futures import (
    ThreadPoolExecutor,
    as_completed,
)
import sqlite3

DATABASE_URL = 'db.sqlite'

# Connect to database.
con = sqlite3.connect(DATABASE_URL)
cur = con.cursor()

# Create a table with a single field.
cur.execute('DROP TABLE IF EXISTS t')
cur.execute('CREATE TABLE t (x INTEGER)')
cur.execute('INSERT INTO t VALUES (0)')
con.commit()


def retry(f, times=3):
    def inner():
        for i in range(times):
            try:
                return f()
            except sqlite3.OperationalError:
                if i == times - 1:
                    raise
                else:
                    continue
    return inner


# In parallel threads, increment the value in the table.
@retry
def update_db_field():
    con = sqlite3.connect(DATABASE_URL, timeout=60)
    con.execute('PRAGMA busy_timeout = 60')
    con.execute('PRAGMA journal_mode = WAL')
    con.execute('PRAGMA synchronous = NORMAL')

    cur = con.cursor()
    value = None
    cur.execute('BEGIN IMMEDIATE')
    value = cur.execute('SELECT x FROM t').fetchone()[0]
    value += 1
    cur.execute('UPDATE t SET x=?', (value,))
    con.commit()

    return value


futures = []
with ThreadPoolExecutor(max_workers=10) as executor:
    for _ in range(1000):
        futures.append(executor.submit(retry(update_db_field)))

results = []
num_errors = 0
for future in as_completed(futures):
    try:
        results.append(future.result())
    except sqlite3.Error:
        num_errors += 1

# Show the results.
print(max(results))
print(con.execute('SELECT x from t').fetchone()[0])
print('num errors =', num_errors)
