#!/usr/bin/env python3
"""Normal-approximation sample-size planning; no claim of observed power."""
from __future__ import annotations
import argparse, json, math
from statistics import NormalDist

def main()->None:
    ap=argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--discordance',type=float,default=0.15)
    ap.add_argument('--pilot-total',type=int)
    ap.add_argument('--pilot-discordant',type=int)
    args=ap.parse_args()
    if not 0<args.discordance<=1: ap.error('discordance must be in (0,1]')
    z=NormalDist().inv_cdf(.95); zpow=NormalDist().inv_cdf(.8)
    q=args.discordance; pilot=None
    if args.pilot_total is not None or args.pilot_discordant is not None:
        n=args.pilot_total; k=args.pilot_discordant
        if n is None or k is None or n<1 or not 0<=k<=n: ap.error('valid paired pilot counts are required')
        phat=k/n
        upper=(phat+z*z/(2*n)+z*math.sqrt(phat*(1-phat)/n+z*z/(4*n*n)))/(1+z*z/n)
        q=max(q,upper)
        pilot={'tasks':n,'discordant':k,'one_sided_95_wilson_upper':upper}
    raw=(z+zpow)**2*q/(.05**2)
    rounded=max(400,math.ceil(raw/20)*20)
    print(json.dumps({'pilot':pilot,'q_used':q,'margin':.05,'one_sided_alpha':.05,'planning_power':.8,
        'normal_approximation_n':math.ceil(raw),'planned_independent_tasks':rounded,
        'warning':'planning approximation, not measured power; prespecify paired CI and validate its power by simulation before formal launch; cluster repeated seeds by task',
        'freeze_before_formal_run':True},ensure_ascii=False,indent=2))
if __name__=='__main__': main()
