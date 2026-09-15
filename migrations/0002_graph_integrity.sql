CREATE UNIQUE INDEX IF NOT EXISTS entities_owner_type_name_uq
    ON entities(owner_id, entity_type, lower(canonical_name));

CREATE INDEX IF NOT EXISTS entity_aliases_alias_lower_idx
    ON entity_aliases(lower(alias));

CREATE INDEX IF NOT EXISTS relations_owner_validity_idx
    ON relations(owner_id, valid_from, valid_until);
