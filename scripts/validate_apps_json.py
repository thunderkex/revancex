#!/usr/bin/env python3
"""Validate docs/apps.json against docs/apps.schema.json."""
import json
import sys


def main():
    json_path = sys.argv[1] if len(sys.argv) > 1 else "docs/apps.json"
    schema_path = sys.argv[2] if len(sys.argv) > 2 else "docs/apps.schema.json"

    with open(json_path, "r", encoding="utf-8") as f:
        data = json.load(f)
    with open(schema_path, "r", encoding="utf-8") as f:
        schema = json.load(f)

    try:
        import jsonschema
    except ImportError as exc:
        print(f"ERROR: jsonschema package is required: {exc}", file=sys.stderr)
        sys.exit(1)

    jsonschema.validate(instance=data, schema=schema)
    print(
        f"OK: {json_path} matches {schema_path} "
        f"(schema_version={data.get('schema_version')}, "
        f"apps={len(data.get('apps', []))}, "
        f"bundles={len(data.get('bundles', []))})"
    )


if __name__ == "__main__":
    main()
