#!/usr/bin/env python3
"""Describe recorded delivery operations without estimating missing model tokens."""
from __future__ import annotations
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path

RUN_IDS = ('yavo-a', 'rust-a', 'rust-b', 'yavo-b')

def load(path):
    return json.loads(path.read_text(encoding='utf-8'))

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def analyze(root):
    summary = load(root / 'results.json')
    run_summaries = {r['run_id']: r for r in summary['runs']}
    output = {'schema_version': 1, 'runs': [], 'languages': {}, 'ratios_yavo_over_rust': {},
              'notes': [
                  'Operations are instrumented lab requests, not total platform tool calls.',
                  'Wall time is controller init through seal; it includes dispatch, generation, service waiting and tools.',
                  'Outside recorded operation bodies is a residual, not measured reasoning time.',
                  'Source and document bytes are artifact sizes, never token estimates.',
                  'All token and monetary costs remain unavailable.'
              ]}
    for run_id in RUN_IDS:
        directory = root / 'runs' / run_id
        meta = load(directory / 'run.json')
        events = [json.loads(line) for line in (directory / 'events.jsonl').read_text(encoding='utf-8').splitlines() if line]
        assert meta['status'] == 'sealed'
        assert meta['operations'] == len(events)
        compiler = [e['result'] for e in events if e['operation'] == 'build' and 'result' in e]
        execution = [c['observed'] for e in events if e['operation'] in ('public', 'run') for c in e.get('results', [])]
        source_files = [f for t in sorted(meta['tasks']) for f in (directory / 'tasks' / t / 'final').rglob('*') if f.is_file()]
        operation_seconds = sum(e['elapsed_seconds'] for e in events)
        row = {
            'run_id': run_id, 'language': meta['language'], 'replicate': meta['replicate'],
            'run_sha256': digest(directory / 'run.json'), 'events_sha256': digest(directory / 'events.jsonl'),
            'successful_tasks': run_summaries[run_id]['metrics']['final_contract_correct']['passed'],
            'task_count': len(meta['tasks']), 'recorded_lab_operations': len(events),
            'categories': dict(sorted(Counter(e['category'] for e in events).items())),
            'wall_seconds': meta['elapsed_seconds'], 'operation_body_seconds': operation_seconds,
            'outside_operation_bodies_seconds': meta['elapsed_seconds'] - operation_seconds,
            'compiler_process_seconds': sum(r['elapsed_seconds'] for r in compiler),
            'compiler_invocations': len(compiler),
            'compiler_failures': sum(bool(r.get('timed_out', False) or r.get('output_limit', False) or r.get('signal', 0) or r.get('exit_code') != 0) for r in compiler),
            'participant_test_process_seconds': sum(r['elapsed_seconds'] for r in execution),
            'participant_test_processes': len(execution),
            'documentation_reads': dict(sorted(Counter(e['arguments'][0] for e in events if e['operation'] == 'doc').items())),
            'documentation_bytes_returned': sum((directory / 'operations' / f"{e['sequence']:04d}" / 'output.txt').stat().st_size for e in events if e['operation'] == 'doc'),
            'source_bytes_written': sum(e['bytes'] for e in events if e['operation'] == 'write' and 'bytes' in e),
            'final_source_bytes': sum(f.stat().st_size for f in source_files),
            'final_source_lines': sum(len(f.read_bytes().splitlines()) for f in source_files),
            'tokens': None, 'all_model_tool_calls': None, 'monetary_cost': None,
        }
        output['runs'].append(row)
    summed = ('successful_tasks', 'task_count', 'recorded_lab_operations', 'wall_seconds',
              'operation_body_seconds', 'outside_operation_bodies_seconds', 'compiler_process_seconds',
              'compiler_invocations', 'compiler_failures', 'participant_test_process_seconds',
              'participant_test_processes', 'documentation_bytes_returned', 'source_bytes_written',
              'final_source_bytes', 'final_source_lines')
    for language in ('yavo', 'rust'):
        rows = [r for r in output['runs'] if r['language'] == language]
        totals = {key: sum(r[key] for r in rows) for key in summed}
        categories = Counter()
        for row in rows:
            categories.update(row['categories'])
        totals['categories'] = dict(sorted(categories.items()))
        totals['independent_batches'] = len(rows)
        successes = totals['successful_tasks']
        totals['operations_per_successful_task'] = totals['recorded_lab_operations'] / successes if successes else None
        totals['seconds_per_successful_task'] = totals['wall_seconds'] / successes if successes else None
        output['languages'][language] = totals
    for key in ('recorded_lab_operations', 'wall_seconds', 'compiler_process_seconds', 'final_source_bytes',
                'operations_per_successful_task', 'seconds_per_successful_task'):
        a, b = output['languages']['yavo'][key], output['languages']['rust'][key]
        output['ratios_yavo_over_rust'][key] = a / b if a is not None and b else None
    return output

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('experiment', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    result = analyze(args.experiment)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, ensure_ascii=False, sort_keys=True, indent=2) + '\n', encoding='utf-8')
    print(json.dumps({'output': str(args.output), 'languages': result['languages'], 'ratios': result['ratios_yavo_over_rust']}, ensure_ascii=False))

if __name__ == '__main__':
    main()
