"""Owner-authored sonic vocabulary; artwork parameters never grant authority."""
import hashlib
import json
import math
import re

DEFAULT = {
    'schema':'daimon.sonic-expression/v1','id':'source-beacon','revision':'1',
    'author':'Source / CompAII','label':'Source hum',
    'fundamental_hz':65,'wave':'sine','pulse_hz':0.22,
    'attack_ms':100,'release_ms':180,'max_sessions':24,'peak_gain':0.08,
    'intervals':{'working':[1,3],'waiting':[1,7],'blocked':[1,11],
        'goal_idle':[1,2],'passive':[],'unknown':[1,13]},
    'provenance':[
        'Nicolas: authored 1:11 blocked / 1:7 awaiting reply; not universal perceptual claims',
        'Source: 65Hz anchor, finite envelopes and breathing gain; Beacon-derived composition',
        'https://github.com/AlterMundi/harmonic-shaper',
        'https://github.com/AlterMundi/NaturalHarmony/blob/main/SOURCE_OF_TRUTH.md'],
}


def descriptor(config):
    value=config.get('sonic_vocabulary',DEFAULT)
    if not isinstance(value,dict) or value.get('schema')!='daimon.sonic-expression/v1':
        raise ValueError('Unknown sonic vocabulary')
    for key in ('id','revision'):
        if not isinstance(value.get(key),str) or not re.fullmatch(r'[A-Za-z0-9_.-]{1,80}',value[key]):
            raise ValueError('Invalid sonic catalog identity')
    for key in ('author','label'):
        if not isinstance(value.get(key),str) or not 1<=len(value[key])<=128 or any(ord(c)<32 for c in value[key]):
            raise ValueError('Invalid sonic catalog label')
    for key,low,high in (('fundamental_hz',20,256),('pulse_hz',0,4),('attack_ms',20,2000),
                         ('release_ms',20,2000),('peak_gain',0.001,0.2)):
        number=value.get(key)
        if type(number) not in (int,float) or not math.isfinite(number) or not low<=number<=high:
            raise ValueError('Invalid sonic synthesis parameter')
    if value.get('wave') not in ('sine','triangle') or type(value.get('max_sessions')) is not int or not 1<=value['max_sessions']<=32:
        raise ValueError('Invalid sonic renderer limit')
    intervals=value.get('intervals',{})
    if not isinstance(intervals,dict) or set(intervals)!=set(DEFAULT['intervals']):
        raise ValueError('Incomplete sonic state vocabulary')
    for state,ratio in intervals.items():
        if not isinstance(ratio,list) or len(ratio) not in (0,2) or any(type(n) is not int or not 1<=n<=64 for n in ratio):
            raise ValueError('Invalid sonic interval')
    encoded=json.dumps(value,sort_keys=True,separators=(',',':'),ensure_ascii=True,allow_nan=False)
    if len(encoded)>65536:raise ValueError('Sonic vocabulary exceeds its bound')
    return {'vocabulary_json':encoded,'vocabulary_sha256':hashlib.sha256(encoded.encode()).hexdigest()}
