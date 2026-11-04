#!/usr/bin/env python3
"""Real SDK runtime/debug smoke on an explicitly enabled disposable scene.

Run phases prepare / inspect / step / next / continue / play-pause / stop. Prepare creates unsaved
source+binding data and reaches line6; it never saves. Later phases preserve the
fixture so native debugger buttons can be exercised between SDK observations.
"""
import argparse
import asyncio
import base64
import hashlib
import json
import os
from pathlib import Path
import time

from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client

SOURCE = """local function inner(o,dt)
 o.translation[1]=o.translation[1]+dt
 return o
end
return {update=function(o,p,dt,host)
 local result=inner(o,dt)
 result.translation[1]=result.translation[1]+2*dt
 return result
end}
"""


async def main(args):
    binary = Path(__file__).resolve().parents[1] / 'target/debug/ruster-mcp'
    trace = []
    async with stdio_client(StdioServerParameters(command=str(binary), env={**os.environ})) as (read, write):
        async with ClientSession(read, write) as client:
            await client.initialize()
            names = {tool.name for tool in (await client.list_tools()).tools}
            assert len(names) == 44 and {'behavior_debug_next', 'behavior_runtime_reset'} <= names

            async def call(name, value=None, error=None):
                response = await client.call_tool(name, value or {})
                result = response.structured_content
                trace.append({'tool': name, 'arguments': value or {}, 'response': result})
                args.output.write_text(json.dumps({'ok': False, 'phase': args.phase, 'trace': trace}, indent=2) + '\n')
                assert result is not None and bool(response.is_error) == (error is not None), response
                if error:
                    assert result['error']['code'] == error, result
                    return result
                assert result['ok'], result
                return result['result']

            scene = await call('scene_inspect')
            assert {'behavior_debug_next', 'behavior_runtime_stop'} <= set(scene['capabilities'])
            assert not scene['drafts_pending'] and not scene['recovery_pending'], scene
            expected = {'expected_session_token': scene['session_token'], 'expected_revision': scene['revision']}

            async def wait(predicate):
                deadline = time.monotonic() + 12
                while time.monotonic() < deadline:
                    value = await call('behavior_runtime_inspect')
                    if predicate(value):
                        return value
                    await asyncio.sleep(.05)
                raise AssertionError('Runtime observation deadline')

            if args.phase == 'prepare':
                bindings = await call('behavior_inspect', {'kind': 'bindings'})
                owners = {binding['object_id'] for binding in bindings['page']['items']}
                owner = next(obj['id'] for obj in scene['objects'] if obj['id'] not in owners)
                created = await call('behavior_create', {**expected, 'source_id': 'debug-proof', 'binding_id': 'debug-proof',
                    'object_id': owner, 'label': 'Debugger proof', 'language_id': 'lua54', 'host_api_version': 1,
                    'text_base64': base64.b64encode(SOURCE.encode()).decode()})
                expected['expected_revision'] = created['revision']
                point = {**expected, 'source_id': 'debug-proof', 'source_hash': hashlib.sha256(SOURCE.encode()).hexdigest(),
                         'line': '6', 'binding_id': None, 'enabled': True}
                await call('behavior_breakpoint_set', point)
                reset = await call('behavior_runtime_reset', expected)
                state = await wait(lambda value: value['ready'] and not value['busy'])
                receipt = await call('behavior_runtime_inspect', {'operation_id': reset['operation_id']})
                assert receipt['operation']['state'] == 'completed', receipt
                await call('behavior_runtime_step', {**expected, 'expected_run_id': state['run_id']})
                state = await wait(lambda value: value['debug_stop_id'] is not None)
                assert state['tick'] == '0', state
            else:
                state = await call('behavior_runtime_inspect')
            if args.phase == 'step':
                await call('behavior_runtime_step', {**expected, 'expected_run_id': state['run_id']})
                state = await wait(lambda value: value['debug_stop_id'] is not None)
            if args.phase == 'play-pause':
                points = await call('behavior_breakpoints')
                assert points['count'] == 1, points
                point = points['breakpoints'][0]
                await call('behavior_breakpoint_set', {**expected, **point, 'enabled': False})
                tick = int(state['tick'])
                await call('behavior_runtime_play', {**expected, 'expected_run_id': state['run_id']})
                state = await wait(lambda value: value['playing'] and int(value['tick']) > tick)
                paused = await call('behavior_runtime_pause', {**expected, 'expected_run_id': state['run_id']})
                state = await wait(lambda value: not value['playing'] and not value['busy'])
                receipt = await call('behavior_runtime_inspect', {'operation_id': paused['operation_id']})
                assert receipt['operation']['state'] == 'completed', receipt
            if args.phase == 'stop':
                operation = await call('behavior_runtime_stop', {**expected, 'expected_run_id': state['run_id']})
                state = await wait(lambda value: not value['busy'] and not value['visible'])
                receipt = await call('behavior_runtime_inspect', {'operation_id': operation['operation_id']})
                assert receipt['operation']['state'] == 'completed', receipt
            elif state['debug_stop_id']:
                exact = {**expected, 'expected_run_id': state['run_id'], 'expected_stop_id': state['debug_stop_id']}
                page = await call('behavior_debug_inspect', exact)
                assert len(json.dumps(page).encode()) < 60000
                if args.line:
                    assert page['line'] == str(args.line), page
                if args.phase in ('next', 'continue'):
                    await call('behavior_debug_' + args.phase, exact)
                    await call('behavior_debug_continue', exact, error='stale_stop')
                    if args.phase == 'next':
                        state = await wait(lambda value: value['debug_stop_id'] and value['debug_stop_id'] != exact['expected_stop_id'])
                    else:
                        state = await wait(lambda value: value['tick'] != '0' and not value['busy'])
            after = await call('scene_inspect')
            assert after['revision'] == expected['expected_revision'], after
            assert [(obj['id'], obj['transform']) for obj in after['objects']] == [(obj['id'], obj['transform']) for obj in scene['objects']]
            result = {'ok': True, 'phase': args.phase, 'runtime': state, 'scene_revision': after['revision'], 'trace': trace}
    args.output.write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({'ok': True, 'phase': args.phase, 'output': str(args.output), 'tick': state['tick'], 'stop': state['debug_stop_id']}))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('phase', choices=['prepare', 'inspect', 'step', 'next', 'continue', 'play-pause', 'stop'])
    parser.add_argument('--line', type=int)
    parser.add_argument('--output', type=Path, required=True)
    asyncio.run(main(parser.parse_args()))
