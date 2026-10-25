"""Declarative additive sine composition; observed facts are never inferred moods."""
import hashlib
import json
import math
import re

DEFAULT = {
    'schema':'daimon.sonic-expression/v2','id':'source-beacon','revision':'6',
    'author':'Source / CompAII','label':'Source Beacon',
    'fundamental_hz':40.4,'wave':'sine','pulse_hz':0.25,'pulse_depth':0.25,'pulse_mode':'harmonic',
    'attack_ms':750,'release_ms':1000,'max_sessions':24,'peak_gain':0.6,
    'components':{'idle':1,'active':2,'tools':3,'goal':4,'pending':7,'blocked':11},
    'provenance':['Nicolas: 40.4 Hz F1; additive F1 idle, F2 active, F3 tools/subagents, F4 active goal, F7 pending, F11 blockage/error; pure sine waves with gentle amplitude pulses'],
}


def descriptor(config):
    value=config.get('sonic_vocabulary',DEFAULT)
    if not isinstance(value,dict) or value.get('schema')!='daimon.sonic-expression/v2':
        raise ValueError('Expected additive sonic vocabulary v2')
    for key in ('id','revision'):
        if not isinstance(value.get(key),str) or not re.fullmatch(r'[A-Za-z0-9_.-]{1,80}',value[key]):
            raise ValueError('Invalid sonic catalog identity')
    for key in ('author','label'):
        if not isinstance(value.get(key),str) or not 1<=len(value[key])<=128 or any(ord(c)<32 for c in value[key]):
            raise ValueError('Invalid sonic catalog label')
    for key,low,high in (('fundamental_hz',20,256),('pulse_hz',0,1),('pulse_depth',0,0.5),
                         ('attack_ms',20,2000),('release_ms',20,2000),('peak_gain',0.001,0.8)):
        number=value.get(key)
        if type(number) not in (int,float) or not math.isfinite(number) or not low<=number<=high:
            raise ValueError('Invalid sonic synthesis parameter')
    if value.get('wave')!='sine' or type(value.get('max_sessions')) is not int or not 1<=value['max_sessions']<=32:
        raise ValueError('Invalid sine renderer limit')
    if value.get('pulse_mode','shared') not in ('shared','harmonic'):
        raise ValueError('Invalid amplitude pulse mode')
    components=value.get('components')
    if (not isinstance(components,dict) or set(components)!=set(DEFAULT['components'])
            or any(type(n) is not int or not 1<=n<=64 for n in components.values())):
        raise ValueError('Incomplete additive sonic vocabulary')
    if any(key in value for key in ('partials','texture_hz','texture_depth','intervals')):
        raise ValueError('Extra harmonic stacks and texture are not part of this composition')
    encoded=json.dumps(value,sort_keys=True,separators=(',',':'),ensure_ascii=True,allow_nan=False)
    if len(encoded)>65536:raise ValueError('Sonic vocabulary exceeds its bound')
    return {'vocabulary_json':encoded,'vocabulary_sha256':hashlib.sha256(encoded.encode()).hexdigest()}
