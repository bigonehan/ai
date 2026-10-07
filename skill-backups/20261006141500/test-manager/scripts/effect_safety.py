"""Optional v6 gate for delayed side effects and production dependency provenance.

Checks recorded evidence consistency, not recorder honesty or live-site behavior.
"""
from collections import Counter


def _text(value):
    return isinstance(value, str) and bool(value.strip())


def _integer(value):
    return type(value) is int and value >= 0


def validate_contract(scenario):
    if not isinstance(scenario, dict):
        return ["effect safety scenario must be an object"]
    required = scenario.get("requires_effect_safety", False)
    if type(required) is not bool:
        return ["requires_effect_safety must be boolean"]
    spec = scenario.get("effect_safety")
    if not required and spec is None:
        return []  # Preserve existing v6 receipts.
    if not isinstance(spec, dict):
        return ["effect_safety contract is required"]
    errors = []
    deadlines = spec.get("deadlines_ms")
    horizon = spec.get("observation_ms")
    if not isinstance(deadlines, list) or not deadlines or not all(_integer(v) for v in deadlines):
        errors.append("effect_safety.deadlines_ms must enumerate relevant timer deadlines")
    elif not _integer(horizon) or horizon <= max(deadlines):
        errors.append("effect_safety.observation_ms must exceed every deadline")
    limits = spec.get("limits")
    if not isinstance(limits, dict) or not limits:
        errors.append("effect_safety.limits must define per-logical-request counts, including forbidden effects")
    else:
        for name, bounds in limits.items():
            if not _text(name) or not isinstance(bounds, dict) or not _integer(bounds.get("min")) or not _integer(bounds.get("max")) or bounds["min"] > bounds["max"]:
                errors.append("effect_safety limits require 0 <= min <= max")
    triggers = spec.get("required_triggers")
    if not isinstance(triggers, list) or not triggers or not all(_text(t) for t in triggers) or len(set(triggers)) != len(triggers):
        errors.append("effect_safety.required_triggers must enumerate unique lifecycle events")
    bindings = spec.get("bindings")
    if not isinstance(bindings, list) or not all(_text(b) for b in bindings) or len(set(bindings)) != len(bindings):
        errors.append("effect_safety.bindings must explicitly enumerate tested production dependencies")
    if not isinstance(scenario.get("requirement_ids"), list) or not _text(spec.get("requirement_id")) or spec.get("requirement_id") not in scenario["requirement_ids"]:
        errors.append("effect_safety must reference a declared user requirement")
    gate = spec.get("completion_gate")
    if gate is not None and (not isinstance(gate, dict) or not isinstance(limits, dict) or not _text(gate.get("effect")) or gate.get("effect") not in limits or not _text(gate.get("required_state"))):
        errors.append("completion_gate requires a counted effect and required state")
    return errors


def validate_observation(scenario, observation, read_artifact):
    errors = validate_contract(scenario)
    if errors or scenario.get("effect_safety") is None:
        return errors
    if not isinstance(observation, dict):
        return ["effect safety observation must be an object"]
    spec = scenario["effect_safety"]
    capture = read_artifact(observation.get("effect_safety_capture"), errors, "effect_safety_capture", parse=True)
    if not isinstance(capture, dict):
        return errors + ["effect safety needs an actual event capture, not a success flag"]
    for key in ("scenario_id", "run_id", "phase"):
        if not _text(observation.get(key)) or capture.get(key) != observation.get(key):
            errors.append(f"effect safety capture {key} does not match observation")
    expected_kind = "unit_capture" if observation.get("phase") == "unit" else "runtime_capture"
    if capture.get("evidence_kind") != expected_kind:
        errors.append("effect safety capture cannot promote unit/build evidence to runtime")
    horizon = capture.get("observed_ms")
    if not _integer(horizon) or horizon < spec["observation_ms"]:
        errors.append("effect safety observation ended before delayed effects could occur")
    identities = capture.get("logical_request_ids")
    if not isinstance(identities, list) or not identities or not all(_text(i) for i in identities) or len(set(identities)) != len(identities):
        return errors + ["effect capture requires stable, unique logical request IDs"]
    if capture.get("monitored_effects") != sorted(spec["limits"]):
        errors.append("every effect, including forbidden effects, must be instrumented")
    events = capture.get("events")
    triggers = capture.get("triggers")
    if not isinstance(events, list) or not isinstance(triggers, list):
        return errors + ["effect capture requires ordered events and executed triggers"]
    behavior_errors = []
    counts = Counter()
    previous = -1
    for event in events:
        if not isinstance(event, dict):
            errors.append("effect event must be an object")
            continue
        at = event.get("at_ms")
        identity, effect = event.get("logical_request_id"), event.get("effect")
        if not _integer(at) or at < previous or not _integer(horizon) or at > horizon:
            errors.append("effect event time is invalid or outside observation")
        else:
            previous = at
        if not _text(identity) or not _text(effect) or identity not in identities or effect not in spec["limits"]:
            errors.append("effect event has an undeclared identity or effect")
            continue
        counts[identity, effect] += 1
        gate = spec.get("completion_gate")
        if gate and effect == gate["effect"] and event.get("state_before") != gate["required_state"]:
            behavior_errors.append("next stage started without the user-required completion state")
    for identity in identities:
        for effect, bounds in spec["limits"].items():
            if not bounds["min"] <= counts[identity, effect] <= bounds["max"]:
                behavior_errors.append(f"{identity}: {effect} count violates user contract")
    observed_triggers = set()
    for trigger in triggers:
        if not isinstance(trigger, dict) or not _text(trigger.get("name")) or not _integer(trigger.get("at_ms")) or not _integer(horizon) or trigger["at_ms"] > horizon:
            errors.append("lifecycle trigger needs its actual execution time")
        else:
            identity = trigger.get("logical_request_id")
            if not _text(identity) or identity not in identities:
                errors.append("lifecycle trigger requires a declared logical request ID")
                continue
            observed_triggers.add((identity, trigger["name"]))
            if horizon - trigger["at_ms"] <= max(spec["deadlines_ms"]):
                errors.append("observation must extend past every deadline after each lifecycle trigger")
    if not {(i, t) for i in identities for t in spec["required_triggers"]}.issubset(observed_triggers):
        errors.append("required retry/alarm/restart path was not executed")
    bindings = capture.get("bindings")
    if not isinstance(bindings, list) or any(not isinstance(b, dict) for b in bindings):
        return errors + ["binding capture must enumerate production dependency provenance"]
    names = [b.get("name") for b in bindings]
    if any(not _text(n) for n in names) or len(names) != len(set(names)) or set(names) != set(spec["bindings"]):
        errors.append("binding capture differs from the declared consumed dependencies")
    for binding in bindings:
        if binding.get("origin") != "production" or binding.get("injected") is not False:
            errors.append("a test-supplied dependency cannot repair a missing production binding")
        lineage = scenario.get("input_lineage")
        if not isinstance(lineage, dict):
            errors.append("binding provenance requires frozen input_lineage")
            lineage = {}
        loader = binding.get("loader")
        read_artifact(loader, errors, "binding production loader")
        sources = lineage.get("loader_sources")
        if not isinstance(sources, list) or loader not in sources:
            errors.append("binding loader is outside frozen loader_sources")
        producer = binding.get("producer")
        resolution = binding.get("resolution", "resolved")
        if resolution not in ("missing", "resolved"):
            errors.append("binding resolution must be missing or resolved")
        if resolution == "missing":
            behavior_errors.append("production dependency is unresolved")
            if producer is not None:
                errors.append("a missing binding must have a null producer")
        else:
            read_artifact(producer, errors, "binding producer")
            order = lineage.get("artifact_order")
            if not isinstance(order, list) or producer not in order:
                errors.append("binding producer is outside frozen artifact_order")
    if not isinstance(capture.get("uncaught_errors"), list):
        errors.append("production execution must have an explicit error ledger")
    elif capture["uncaught_errors"]:
        behavior_errors.append("production execution has uncaught errors")
    if observation.get("phase") == "pre_fix" and observation.get("outcome") == "failure":
        if not behavior_errors:
            errors.append("pre-fix failure must demonstrate an effect or runtime error violation")
    else:
        errors.extend(behavior_errors)
    return errors
