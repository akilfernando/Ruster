"""Separate release validation activation/shutdown evidence, not a timing run."""
import fcntl,json,os,subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parent
run=ROOT/'evidence/guarded-edit/validation';run.mkdir(parents=True,exist_ok=True)
with open('/home/akilf/.hermes/qa/rusterize/state/gate.lock','a') as gate:
    fcntl.flock(gate,fcntl.LOCK_EX|fcntl.LOCK_NB)
    env={k:v for k,v in os.environ.items() if not k.startswith(('RUSTER_','EGUI_INSPECTION'))}
    env.update(VK_LAYER_PATH='/home/akilf/.local/opt/vulkan-validation-layers-1.4.363.1-1/scoped-layer',
               VK_INSTANCE_LAYERS='VK_LAYER_KHRONOS_validation',VK_LAYER_VALIDATE_SYNC='1',VK_LOADER_DEBUG='layer',
               VK_LAYER_REPORT_FLAGS='error,warn,info,perf,verbose',VK_LAYER_DEBUG_ACTION='VK_DBG_LAYER_ACTION_LOG_MSG',RUSTER_MSAA_SAMPLES='4')
    result=subprocess.run([str(ROOT/'target/release/ruster-localhost-spike'),str(run),'--grant-disposable-edit-undo'],input='render\nrender\n',text=True,capture_output=True,env=env,timeout=30)
    (run/'native.log').write_text(result.stderr);(run/'stdout.jsonl').write_text(result.stdout)
    assert result.returncode==0,result.stderr[-3000:]
    combined = result.stdout + result.stderr
    assert 'CURRENT-VALIDATION-ENABLED' in combined,combined[-4000:]
    assert 'Synchronization' in combined and 'Core' in combined,combined[-4000:]
    (run/'result.json').write_text(json.dumps({'exit':result.returncode,'activation_message':True,'core_and_synchronization_reported':True,'scope':'separate direct-layer logging run; not included in timing cohort'},indent=2))
    print(combined[combined.index('CURRENT-VALIDATION-ENABLED'):][:1000])
