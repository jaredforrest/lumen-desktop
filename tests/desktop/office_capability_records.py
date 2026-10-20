"""Produce separate scoped provider records from immutable office evidence reports."""
import argparse,json
from office_fixture.records import generate
p=argparse.ArgumentParser();p.add_argument('--cli',required=True);p.add_argument('--gui',action='append',default=[]);p.add_argument('--output',required=True);a=p.parse_args()
with open(a.output,'x') as output:json.dump(generate(a.cli,a.gui),output,indent=2);output.write('\n')
