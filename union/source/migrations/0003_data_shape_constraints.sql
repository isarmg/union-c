-- Add database-level data shape constraints for values already validated by the API.

CREATE OR REPLACE FUNCTION union_valid_json_object(value TEXT)
RETURNS BOOLEAN AS $$
BEGIN
    RETURN jsonb_typeof(value::jsonb) = 'object';
EXCEPTION WHEN others THEN
    RETURN FALSE;
END;
$$ LANGUAGE plpgsql IMMUTABLE STRICT;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM blog_posts
        WHERE length(trim(relative_path)) = 0
           OR relative_path !~ E'\\.mdx?$'
           OR relative_path ~ E'(^/|(^|/)\\.\\.?(/|$)|\\\\|//)'
           OR length(trim(title)) = 0
           OR length(trim(COALESCE(description, ''))) = 0
    ) THEN
        RAISE EXCEPTION 'cannot add blog post shape constraints: invalid blog_posts rows exist';
    END IF;

    IF EXISTS (
        SELECT 1 FROM blog_taxonomy
        WHERE length(trim(name)) = 0
           OR length(name) > 64
           OR name ~ E'[\\n\\r,，]'
    ) THEN
        RAISE EXCEPTION 'cannot add taxonomy name constraints: invalid blog_taxonomy rows exist';
    END IF;

    IF EXISTS (
        SELECT 1 FROM blog_post_tags
        WHERE length(trim(tag)) = 0
           OR length(tag) > 64
           OR tag ~ E'[\\n\\r,，]'
    ) THEN
        RAISE EXCEPTION 'cannot add blog tag constraints: invalid blog_post_tags rows exist';
    END IF;

    IF EXISTS (
        SELECT 1 FROM blog_category_tags
        WHERE length(trim(category)) = 0
           OR length(category) > 64
           OR category ~ E'[\\n\\r,，]'
           OR length(trim(tag)) = 0
           OR length(tag) > 64
           OR tag ~ E'[\\n\\r,，]'
    ) THEN
        RAISE EXCEPTION 'cannot add category tag constraints: invalid blog_category_tags rows exist';
    END IF;

    IF EXISTS (
        SELECT 1 FROM external_hosts
        WHERE length(trim(host_id)) = 0
           OR NOT union_valid_json_object(config)
           OR secret = ''
    ) THEN
        RAISE EXCEPTION 'cannot add external host constraints: invalid external_hosts rows exist';
    END IF;
END
$$;

ALTER TABLE blog_posts
    ADD CONSTRAINT blog_posts_relative_path_shape_check
    CHECK (
        length(trim(relative_path)) > 0
        AND relative_path ~ E'\\.mdx?$'
        AND relative_path !~ E'(^/|(^|/)\\.\\.?(/|$)|\\\\|//)'
    ),
    ADD CONSTRAINT blog_posts_title_description_check
    CHECK (
        length(trim(title)) > 0
        AND length(trim(COALESCE(description, ''))) > 0
    );

ALTER TABLE blog_taxonomy
    ADD CONSTRAINT blog_taxonomy_name_shape_check
    CHECK (
        length(trim(name)) > 0
        AND length(name) <= 64
        AND name !~ E'[\\n\\r,，]'
    );

ALTER TABLE blog_post_tags
    ADD CONSTRAINT blog_post_tags_tag_shape_check
    CHECK (
        length(trim(tag)) > 0
        AND length(tag) <= 64
        AND tag !~ E'[\\n\\r,，]'
    );

ALTER TABLE blog_category_tags
    ADD CONSTRAINT blog_category_tags_shape_check
    CHECK (
        length(trim(category)) > 0
        AND length(category) <= 64
        AND category !~ E'[\\n\\r,，]'
        AND length(trim(tag)) > 0
        AND length(tag) <= 64
        AND tag !~ E'[\\n\\r,，]'
    );

ALTER TABLE external_hosts
    ADD CONSTRAINT external_hosts_host_id_shape_check
    CHECK (length(trim(host_id)) > 0),
    ADD CONSTRAINT external_hosts_config_json_object_check
    CHECK (union_valid_json_object(config)),
    ADD CONSTRAINT external_hosts_secret_not_empty_check
    CHECK (secret IS NULL OR length(secret) > 0);
