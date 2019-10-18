CREATE TABLE hash(
    id SERIAL PRIMARY KEY,
    hash BYTEA NOT NULL UNIQUE
);

CREATE TABLE hash_tag(
    hash_id INTEGER NOT NULL REFERENCES hash(id),
    tag_id INTEGER NOT NULL REFERENCES tag(id),
    PRIMARY KEY (hash_id, tag_id)
);

CREATE TABLE tag(
    id SERIAL PRIMAY KEY,
    name TEXT NOT NULL UNIQUE
);
