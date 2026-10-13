"""Validate the pinned, closed Daimon Alerts v1 projection without new dependencies.

Only the keywords used by this pinned schema are supported. A schema extension
fails closed until the consumer is qualified against that version.
"""
import json
from pathlib import Path
import re

SCHEMA = json.loads((Path(__file__).parent / 'contracts/daimon-alerts-v1.json').read_text())
KEYWORDS = {'$ref', 'type', 'const', 'enum', 'properties', 'additionalProperties',
            'required', 'items', 'pattern', 'minLength', 'maxLength', 'minimum',
            'maximum', 'anyOf'}


def validate(value, name):
    def check(item, spec, depth=0):
        if depth > 24 or set(spec) - KEYWORDS:
            raise ValueError('Unsupported contract shape')
        if '$ref' in spec:
            ref = spec['$ref']
            if not ref.startswith('#/$defs/'):
                raise ValueError('Unsupported contract reference')
            return check(item, SCHEMA['$defs'][ref.removeprefix('#/$defs/')], depth + 1)
        if 'anyOf' in spec:
            for alternative in spec['anyOf']:
                try:
                    check(item, alternative, depth + 1)
                    return
                except ValueError:
                    pass
            raise ValueError('Invalid contract alternative')
        if 'const' in spec and item != spec['const']:
            raise ValueError('Invalid contract version')
        if 'enum' in spec and item not in spec['enum']:
            raise ValueError('Invalid contract value')
        kind = spec.get('type')
        kinds = kind if isinstance(kind, list) else [kind]
        types = {'object': dict, 'array': list, 'string': str, 'integer': int,
                 'boolean': bool, 'null': type(None)}
        if kind is not None and type(item) not in [types[k] for k in kinds]:
            raise ValueError('Invalid contract type')
        if isinstance(item, dict):
            properties = spec.get('properties', {})
            if set(spec.get('required', [])) - set(item) or (
                    spec.get('additionalProperties') is False and set(item) - set(properties)):
                raise ValueError('Invalid contract fields')
            for key, child in item.items():
                check(child, properties[key], depth + 1)
        elif isinstance(item, list):
            if len(item) > 2000:
                raise ValueError('Contract collection exceeds consumer bound')
            for child in item:
                check(child, spec['items'], depth + 1)
        elif isinstance(item, str):
            if not spec.get('minLength', 0) <= len(item) <= spec.get('maxLength', 4096):
                raise ValueError('Contract string exceeds bound')
            if 'pattern' in spec and not re.search(spec['pattern'], item):
                raise ValueError('Invalid contract string')
        elif type(item) is int:
            if item < spec.get('minimum', 0) or item > spec.get('maximum', 2**63 - 1):
                raise ValueError('Contract integer exceeds bound')
    check(value, SCHEMA['$defs'][name])
    return value
