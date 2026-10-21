"""Owner-authored sonic vocabulary; artwork parameters never grant authority."""
import hashlib
import json
import math
import re

DEFAULT = {
    'schema':'daimon.sonic-expression/v1','id':'source-beacon','revision':'2',
    'author':'Source / CompAII','label':'Source hum',
    'fundamental_hz':65,'wave':'sine','pulse_hz':0.22,
    'attack_ms':180,'release_ms':350,'max_sessions':24,'peak_gain':0.08,
    'partials':[[1,0.64],[2,0.26],[3,0.10]],
    'texture_hz':18,'texture_depth':0.12,
    'intervals':{'working':[1,3],'waiting':[1,7],'blocked':[1,11],
        'goal_idle':[1,2],'passive':[],'unknown':[1,13]},
    'provenance':[
        'Nicolas: authored 1:11 blocked / 1:7 awaiting reply; not universal perceptual claims',
        'Source: Beacon 65Hz anchor; weighted natural partials, breathing gain and a gentle 18Hz purr',
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
    partials=value.get('partials',[[1,1]])
    if (not isinstance(partials,list) or not 1<=len(partials)<=4
            or any(not isinstance(p,list) or len(p)!=2 or type(p[0]) is not int or not 1<=p[0]<=8
                   or type(p[1]) not in (int,float) or not math.isfinite(p[1]) or not 0<p[1]<=1 for p in partials)
            or len({p[0] for p in partials})!=len(partials) or sum(p[1] for p in partials)>1.000001):
        raise ValueError('Invalid sonic partials')
    for key,high in (('texture_hz',40),('texture_depth',0.25)):
        number=value.get(key,0)
        if type(number) not in (int,float) or not math.isfinite(number) or not 0<=number<=high:
            raise ValueError('Invalid sonic texture')
    encoded=json.dumps(value,sort_keys=True,separators=(',',':'),ensure_ascii=True,allow_nan=False)
    if len(encoded)>65536:raise ValueError('Sonic vocabulary exceeds its bound')
    return {'vocabulary_json':encoded,'vocabulary_sha256':hashlib.sha256(encoded.encode()).hexdigest()}
