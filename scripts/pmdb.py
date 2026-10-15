"""Create or check a Pacemaker music.db against what pmplayer 14142 requires.

  pmdb.py create <music.db>   new empty library, equivalent to pmplayer's formatEmptyDb
  pmdb.py check  <music.db>   compare an existing DB (e.g. a device dump) with the device schema

Rules (notes/FINDINGS.md section 3): user_version exactly 677; tables/columns from the
tl::g_*Fields arrays (notes/schema-14142.sql); indexes mixtracks_idx, tracks_location_idx,
casetracks_idx. Plain SQLite is enough because the ICU collation is only used at query time.
"""
import os, re, sqlite3, sys

USER_VERSION = 677
HERE = os.path.dirname(os.path.abspath(__file__))
SCHEMA = os.path.join(HERE, "..", "notes", "schema-14142.sql")
INDEXES = [
    "CREATE INDEX mixtracks_idx ON mixtracks (track_id)",
    "CREATE INDEX tracks_location_idx ON tracks (location)",
    "CREATE INDEX casetracks_idx ON casetracks (track_id)",
]

def expected():
    """{table: [(column, declared type), ...]} parsed from the generated DDL."""
    ddl = open(SCHEMA, encoding="utf-8").read()
    out = {}
    for name, body in re.findall(r"CREATE TABLE (\w+) \((.*?)\n\);", ddl, re.S):
        out[name] = [tuple(line.strip().rstrip(",").split(" ", 1)) for line in body.strip().splitlines()]
    return out

def create(path):
    if os.path.exists(path):
        sys.exit(f"refusing to overwrite {path}")
    con = sqlite3.connect(path)
    con.executescript(open(SCHEMA, encoding="utf-8").read())
    for ix in INDEXES:
        con.execute(ix)
    con.execute(f"PRAGMA user_version={USER_VERSION}")
    con.commit()
    con.close()

def check(path):
    con = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    problems = []
    uv = con.execute("PRAGMA user_version").fetchone()[0]
    if uv != USER_VERSION:
        problems.append(f"user_version is {uv}, device requires {USER_VERSION}")
    for table, cols in expected().items():
        have = [(r[1], r[2]) for r in con.execute(f"PRAGMA table_info({table})")]
        if not have:
            problems.append(f"missing table {table}")
        elif [c for c, _ in have] != [c for c, _ in cols]:
            problems.append(f"{table}: columns differ\n    device: {[c for c, _ in cols]}\n    db:     {[c for c, _ in have]}")
        else:
            for (c, t_exp), (_, t_have) in zip(cols, have):
                base = t_exp.split(" PRIMARY")[0].split(" DEFAULT")[0]
                if not t_have.upper().startswith(base.upper()):
                    problems.append(f"{table}.{c}: type {t_have!r}, device declares {t_exp!r}")
    idx = {r[0] for r in con.execute("SELECT name FROM sqlite_master WHERE type='index'")}
    for ix in INDEXES:
        name = ix.split()[2]
        if name not in idx:
            problems.append(f"missing index {name}")
    extra = {r[0] for r in con.execute("SELECT name FROM sqlite_master WHERE type='table'")} - set(expected()) - {"sqlite_sequence"}
    if extra:
        problems.append(f"extra tables (not in device schema): {sorted(extra)}")
    counts = {t: con.execute(f"SELECT COUNT(*) FROM {t}").fetchone()[0] for t in expected() if (t,) in con.execute("SELECT name FROM sqlite_master WHERE type='table'").fetchall()}
    print("rows:", counts)
    print("OK: matches device schema" if not problems else "PROBLEMS:\n  " + "\n  ".join(problems))
    return not problems

if __name__ == "__main__":
    mode, path = sys.argv[1], sys.argv[2]
    if mode == "create":
        create(path)
        print(f"created {path}")
    else:
        sys.exit(0 if check(path) else 1)
