# External API Consumer Contract

Use this reference when a scenario calls or consumes an external API, including a localhost API owned by another application or an endpoint reached across Windows, WSL, container, VM, or browser boundaries.

## Observation boundary

Trace one stable scenario through:

`initiating action -> request producer -> transport environment -> live response -> response parser -> production consumer -> authoritative readback`

Keep these evidence levels separate:

- A read-only live probe establishes the provider's observed response contract in one named environment.
- A capture-derived fixture establishes deterministic parser and consumer behavior.
- A production-consumer mutant proves the test rejects a wrong field name or type.
- A successful runtime receipt establishes that the live response reached the authoritative consumer.

The first three do not replace the fourth.

## Contract shape

Every contract declares:

```json
{
  "external_api_validation": {
    "applicable": true,
    "reason": "The connection check consumes the live Obsidian status response.",
    "contracts": [
      {
        "contract_id": "obsidian-status",
        "scenario_ids": ["settings-connection-check"],
        "system": "Obsidian Local REST API",
        "operation": "read server status",
        "method": "GET",
        "endpoint_pattern": "https://127.0.0.1:27124/",
        "authoritative_environment": "windows_host",
        "response_schema": [
          {"path": "$.status", "type": "string", "required": true, "safe_value": "OK"},
          {"path": "$.authenticated", "type": "boolean", "required": true, "safe_value": true}
        ],
        "safe_probe": {
          "available": true,
          "source_kind": "live_read_only",
          "path": "/absolute/project/test/tmp/obsidian-status-capture.json",
          "sha256": "<sha256>"
        },
        "fixtures": [
          {
            "path": "/absolute/project/test/fixtures/obsidian-status.json",
            "sha256": "<sha256>",
            "source_capture_sha256": "<capture-sha256>",
            "response_schema": [
              {"path": "$.status", "type": "string", "required": true, "safe_value": "OK"},
              {"path": "$.authenticated", "type": "boolean", "required": true, "safe_value": true}
            ]
          }
        ],
        "consumer": {
          "path": "/absolute/project/src/obsidian-client.js",
          "sha256": "<sha256>",
          "expression": "body.status === 'OK' && body.authenticated === true",
          "required_fields": ["$.status", "$.authenticated"]
        },
        "mutation_tests": [
          {
            "path": "/absolute/project/test/tmp/obsidian-status-name-mutant.json",
            "sha256": "<sha256>"
          },
          {
            "path": "/absolute/project/test/tmp/obsidian-status-type-mutant.json",
            "sha256": "<sha256>"
          }
        ]
      }
    ]
  },
  "runtime_target": {
    "external_api_environments": ["windows_host"]
  },
  "acceptance_scenarios": [
    {
      "scenario_id": "settings-connection-check",
      "external_api_contract_ids": ["obsidian-status"]
    }
  ]
}
```

The scenario-to-contract links are bidirectional. `runtime_target.external_api_environments` must exactly equal the environments named by applicable API contracts.

## Sanitized capture

The file referenced by `safe_probe` is a schema capture, not a raw HTTP dump. It repeats `contract_id`, `system`, `operation`, `method`, `endpoint_pattern`, `authoritative_environment`, `source_kind`, and `response_schema`, and declares `contains_secrets: false`.

Allowed response entries contain only `path`, `type`, `required`, and an optional non-sensitive `safe_value`. Do not include authorization data, API keys, tokens, passwords, cookies, headers, request or response bodies, raw payloads, personal file content, or endpoint query values.

When `safe_probe.available` is true, use `source_kind: live_read_only` and an intrinsically read-only `GET`, `HEAD`, or `OPTIONS` operation. When no safe probe exists, use `available: false` with `source_kind: recorded_response` or `authoritative_docs`. That lower layer can design fixtures but cannot satisfy successful live runtime completion.

## Fixture and mutant lineage

Each fixture records the exact capture hash and the same response schema. The unit observation must list the fixture path and hash, set `boundary_source: capture_fixture`, set `production_consumer_executed: true`, and report `mutations_rejected: ["field_name", "field_type"]`.

The field-name mutation receipt contains `mutation_kind: field_name`, the API contract ID, an existing `from_path`, a different undeclared `to_path`, `production_consumer_executed: true`, `rejected: true`, and a nonzero `exit_code`. The field-type receipt uses `mutation_kind: field_type`, an existing `path`, and different `from_type` and `to_type` values. Both are mandatory. For the example above, changing `$.status` to `$.ok` and `$.authenticated` from `boolean` to `string` must each make the production-consumer test fail.

## Runtime observation

A successful post-fix observation for every linked API includes:

```json
{
  "contract_id": "obsidian-status",
  "authoritative_environment": "windows_host",
  "capture_sha256": "<capture-sha256>",
  "response_schema": ["<exact contract schema>"],
  "production_consumer_executed": true,
  "boundary_source": "live_response",
  "mocked": false,
  "consumer_readback": true
}
```

Do not put raw response values or secrets in runtime evidence. If the production consumer or authoritative readback is not observed, report the scenario as `runtime-unverified` even when the probe, fixture, HTTP status, or parser test passes.
