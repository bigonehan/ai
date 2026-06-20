---
name: effect-domain
description: Use when designing or implementing TypeScript domain models, bounded contexts, use cases, repositories, ports, services, or domain tests in projects that use Effect TS, effect/Schema, Effect.Service, Context, Layer, or typed Effect errors.
---

# Effect Domain

## Overview

Build the domain as Effect-native TypeScript: explicit schemas at boundaries, pure domain rules in small modules, typed errors, ports for external dependencies, and Effect services/layers for composition.

## First Pass

1. Inspect the project's Effect version, imports, and local conventions before choosing APIs. Prefer patterns already used in the repository.
2. Identify the bounded context and write its ubiquitous language as code names: entity, value object, command, event, policy, repository, service.
3. Separate domain code from infrastructure. Domain modules may define ports; adapters implement them elsewhere.
4. Make invalid states unrepresentable where practical. Use branded/refined schemas for value objects and tagged unions for lifecycle states.
5. Return `Effect.Effect<Success, DomainError, Requirements>` from use cases that can fail or require dependencies. Keep pure constructors and predicates as plain functions when no Effect is needed.

## Module Shape

Prefer this structure unless the repository has a stronger local convention:

```text
src/<context>/
  domain/
    <Entity>.ts
    <ValueObject>.ts
    errors.ts
    events.ts
    policies.ts
  application/
    <UseCase>.ts
    ports.ts
  infrastructure/
    <Adapter>.ts
    layers.ts
  index.ts
```

Use `domain/` for business invariants and vocabulary. Use `application/` for orchestration and ports. Use `infrastructure/` for database, HTTP, queues, clocks, IDs, and other external effects.

## Modeling Rules

- Define external input/output with `effect/Schema`; decode at boundaries before the domain sees data.
- Represent value objects with schemas plus small constructors. Do not pass raw strings for important concepts such as email, account id, money, status, or date ranges.
- Use tagged errors for expected domain failures. Do not throw for business-rule failures.
- Keep repositories as ports returning `Effect`; do not import database clients into domain or application use cases.
- Use `Effect.Service`, `Context.Tag`, or the repository's established service style for dependencies. Provide implementations with `Layer`.
- Preserve the error channel. Avoid `Effect.orDie`, untyped `catch`, and blanket error mapping unless crossing an application boundary.
- Prefer domain events for observable facts that occurred; name them in past tense.
- Make tests prove invariants, decoding failures, and use-case dependency behavior with test doubles/layers.

## Implementation Pattern

Use this as a compact target shape, adapting names and APIs to the local Effect version:

```ts
import { Data, Effect, Schema } from "effect"

export const Email = Schema.String.pipe(
  Schema.pattern(/^[^@\s]+@[^@\s]+\.[^@\s]+$/),
  Schema.brand("Email")
)
export type Email = Schema.Schema.Type<typeof Email>

export class DuplicateUserEmail extends Data.TaggedError("DuplicateUserEmail")<{
  readonly email: Email
}> {}

export interface UserRepository {
  readonly existsByEmail: (email: Email) => Effect.Effect<boolean, never>
  readonly save: (user: User) => Effect.Effect<void, never>
}

export class UserRepositoryTag extends Effect.Tag("UserRepository")<
  UserRepositoryTag,
  UserRepository
>() {}

export const registerUser = (input: unknown) =>
  Effect.gen(function* () {
    const email = yield* Schema.decodeUnknown(Email)(input)
    const repo = yield* UserRepositoryTag
    const exists = yield* repo.existsByEmail(email)
    if (exists) return yield* Effect.fail(new DuplicateUserEmail({ email }))
    return yield* repo.save(User.create({ email }))
  })
```

Check this example against the installed Effect version before copying it. If the project uses a different style for service tags, schema decoding, or data constructors, follow that style.

## Review Checklist

- Domain vocabulary appears in type and function names.
- Every use case states success, typed failure, and requirements through `Effect`.
- Boundary validation is explicit and test-covered.
- Business errors are distinct from infrastructure errors.
- Ports are stable interfaces; adapters are replaceable layers.
- No infrastructure import leaks into `domain/`.
- Tests cover at least one happy path, one domain failure, and one invalid input per use case.

## Current Docs

When unsure about Effect APIs, check the current official documentation first:

- Services and `Context`: https://effect.website/docs/requirements-management/services/
- Layers: https://effect.website/docs/requirements-management/layers/
- Schema: https://effect.website/docs/schema/introduction/
