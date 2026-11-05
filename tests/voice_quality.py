"""Same-input quiet-speech controls for the production Rust DSP probe. Never plays audio."""
import argparse
import copy
import hashlib
import json
import math
import sys
import wave
from pathlib import Path

import numpy as np
from scipy import signal

sys.path.insert(0, str(Path(__file__).resolve().parent))
from chain_quality import RATE, rms, save, voice_metrics, write_wav

WINDOW = 480  # Non-overlapping 10 ms, two consecutive windows define an event.


def db(value):
    return 20 * math.log10(max(float(value), 1e-12))


def power_windows(x):
    return np.sqrt(np.mean(x[:len(x)//WINDOW*WINDOW].reshape(-1, WINDOW).astype(float)**2, axis=1))


def prepare(root, speech, presets_path):
    root.mkdir(parents=True, exist_ok=False)
    with wave.open(str(speech)) as wav:
        assert (wav.getframerate(), wav.getnchannels(), wav.getsampwidth()) == (RATE, 1, 2)
        spoken = np.frombuffer(wav.readframes(wav.getnframes()), '<i2').astype(float)/32768
    onset = np.flatnonzero(abs(spoken) > .01)[0]
    spoken = spoken[onset:onset+3*RATE]
    spoken *= 10**(-30/20)/rms(spoken)
    rng = np.random.default_rng(1105)
    noise = lambda n: rng.normal(0, 10**(-65/20), n)
    tail = spoken.copy()
    tail *= 10**(np.r_[np.zeros(RATE), np.linspace(0, -32, 2*RATE)]/20)
    t = np.arange(2*RATE)/RATE
    vowel = sum(np.sin(2*np.pi*150*k*t)/k for k in range(1, 8))
    vowel *= 10**(-30/20)/rms(vowel)
    vowel *= np.minimum(1, np.minimum(t*50, (2-t)*50))
    hiss = signal.sosfilt(signal.butter(4, [4000, 10000], fs=RATE, btype='bandpass', output='sos'), rng.normal(size=RATE))
    hiss *= 10**(-42/20)/rms(hiss)
    quiet_onset = vowel[:RATE]*10**(-26/20)
    steady_tail = vowel*10**(np.linspace(0, -38, len(vowel))/20)
    pieces = [('silence', np.zeros(19200)), ('quiet_onset', quiet_onset),
              ('onset_gap', np.zeros(12000)), ('speech', spoken)]
    for level in (-42, -50, -56):
        pieces += [(f'gap_{level}', np.zeros(12000)),
                   (f'quiet_{level}', spoken*10**((level+30)/20))]
    pieces += [('word_tail', tail), ('steady_tail', steady_tail), ('vowel_150hz', vowel),
               ('sibilants', hiss), ('noise', np.zeros(RATE)), ('drain', np.zeros(2*RATE))]
    segments, masks, data, offset = {}, {}, [], 0
    for name, clean in pieces:
        segments[name] = [offset, offset+len(clean)]
        levels = power_windows(clean)
        if name.startswith('quiet_') or name in ('word_tail', 'steady_tail'):
            # Labels are fixed from the unprocessed signal, before seeing any output.
            active = levels > max(10**(-72/20), levels.max()*10**(-30/20))
            masks[name] = (np.flatnonzero(active)+offset//WINDOW).tolist()
        data.append(clean if name in ('silence', 'drain') else clean+noise(len(clean)))
        offset += len(clean)
    dry = np.concatenate(data).astype('<f4')
    dry.tofile(root/'dry.f32')
    write_wav(root/'stimulus.wav', dry)
    save(root/'segments.json', segments)
    save(root/'labels.json', {'window_frames': WINDOW, 'speech_windows': masks,
                              'listening_frames': [segments['speech'][0], segments['word_tail'][1]]})
    save(root/'source.json', {'speech_file': str(speech), 'speech_sha256': hashlib.sha256(speech.read_bytes()).hexdigest(),
                              'presets_sha256': hashlib.sha256(presets_path.read_bytes()).hexdigest(),
                              'noise_rms_dbfs': -65, 'sample_rate': RATE, 'seconds': len(dry)/RATE})
    presets = json.loads(presets_path.read_text(encoding='utf-8-sig'))
    cases = [{'id': 'raw', 'name': 'raw', 'chain': []}]
    for name in ('female', 'ethereal'):
        chain = next(p['chain'] for p in presets if p['id'] == 'builtin-'+name)
        variants = {'full': chain,
                    'gate': [c for c in chain if c['type'] == 'gate'],
                    'pitch': [c for c in chain if c['type'] == 'pitch'],
                    'voice': [c for c in chain if c['type'] in ('pitch', 'formant')],
                    'no_gate': [c for c in chain if c['type'] != 'gate'],
                    'no_dynamic': [c for c in chain if c['type'] not in ('compressor', 'reverb')]}
        for variant, cards in variants.items():
            label = f'{name}-{variant}'
            cases.append({'id': label, 'name': label, 'chain': cards})
    save(root/'cases.json', cases)


def events(levels, reference, indices):
    indices = np.asarray(indices, dtype=int)
    indices = indices[indices < min(len(levels), len(reference))]
    delta = 20*np.log10(np.maximum(levels[indices], 1e-12)/np.maximum(reference[indices], 1e-12))
    failed = indices[delta < -6]
    groups = np.split(failed, np.flatnonzero(np.diff(failed) != 1)+1)
    groups = [g for g in groups if len(g) >= 2]
    hard = indices[(levels[indices] < 1e-8) & (reference[indices] > 10**(-72/20))]
    return {'events': len(groups), 'duration_ms': sum(len(g)*10 for g in groups),
            'first_s': float(groups[0][0]/100) if groups else None,
            'intervals_s': [[float(g[0]/100), float((g[-1]+1)/100)] for g in groups],
            'hard_silence_ms': len(hard)*10,
            'p05_relative_db': float(np.percentile(delta, 5)) if len(delta) else None}


def outputs(root):
    result, metadata = {}, {}
    for path in root.glob('*.json'):
        item = json.loads(path.read_text(encoding='utf-8-sig'))
        if isinstance(item, dict) and 'dsp_ns' in item:
            data = np.fromfile(path.with_suffix('.f32'), '<f4')
            assert np.isfinite(data).all()
            result[path.stem] = data[item['algorithmic_frames']:]
            metadata[path.stem] = item
    return result, metadata


def summarize(root):
    if (root/'summary.json').exists():
        raise FileExistsError('choose a fresh result directory')
    segments = json.loads((root/'segments.json').read_text())
    labels = json.loads((root/'labels.json').read_text())
    waves, metadata = outputs(root)
    result = {}
    for name, data in waves.items():
        times = np.asarray(metadata[name]['dsp_ns'])/1000
        result[name] = {'peak': float(abs(data).max()), 'over_full_scale': int(np.sum(abs(data)>1)),
                        'dsp_p99_us': float(np.percentile(times, 99)), 'dsp_max_us': float(times.max()),
                        'algorithmic_frames': metadata[name]['algorithmic_frames']}
        result[name]['sections_dbfs'] = {key: db(rms(data[a:b])) for key, (a, b) in segments.items() if a < len(data)}
        if name == 'raw':
            continue
        preset, variant = name.split('-', 1)
        ref_name = preset+'-no_gate' if variant not in ('gate', 'pitch', 'voice') else 'raw'
        reference = power_windows(waves[ref_name])
        levels = power_windows(data)
        result[name]['attenuation_reference'] = ref_name
        result[name]['attenuation'] = {section: events(levels, reference, idx) for section, idx in labels['speech_windows'].items()}
        if variant != 'gate':
            result[name]['pitch'] = voice_metrics(data, segments, 5 if preset == 'female' else 3)
    save(root/'summary.json', result)
    print(json.dumps({name: value for name, value in result.items() if name.endswith(('-full', '-gate'))}, indent=2))


def candidates(source, target):
    target.mkdir(parents=True, exist_ok=False)
    for name in ('dry.f32', 'segments.json', 'labels.json', 'source.json'):
        (target/name).write_bytes((source/name).read_bytes())
    cases = json.loads((source/'cases.json').read_text())
    selected = [c for c in cases if c['id'] == 'raw' or c['id'].endswith(('-full', '-no_gate'))]
    for baseline in [c for c in cases if c['id'].endswith('-full')]:
        for threshold, hold, release in [(-56, 60, 120), (-60, 60, 120), (-56, 120, 180), (-60, 120, 180)]:
            case = copy.deepcopy(baseline)
            case['id'] = case['name'] = baseline['id'].replace('full', f't{abs(threshold)}h{hold}r{release}')
            gate = next(c for c in case['chain'] if c['type'] == 'gate')
            gate['params'].update(threshold_db=threshold, hold_ms=hold, release_ms=release)
            selected.append(case)
    save(target/'cases.json', selected)


def listening(root, candidate):
    import torch
    from torchaudio.functional import loudness
    torch.set_num_threads(1)
    waves, _ = outputs(root)
    start, end = json.loads((root/'labels.json').read_text())['listening_frames']
    key, measured = {}, {}
    for preset, order in [('female', ['full', candidate]), ('ethereal', [candidate, 'full'])]:
        clips = []
        for version in order:
            clip = waves[f'{preset}-{version}'][start:end].copy()
            integrated = float(loudness(torch.from_numpy(clip)[None], RATE))
            clips.append(clip*10**((-23-integrated)/20))
        common = min(1., .95/max(abs(c).max() for c in clips))
        for letter, version, clip in zip('AB', order, clips):
            path = root/f'{preset}-{letter}.wav'
            if path.exists():
                raise FileExistsError(path)
            write_wav(path, clip*common)
            with wave.open(str(path)) as wav:
                pcm = np.frombuffer(wav.readframes(wav.getnframes()), '<i2').astype('float32')/32768
            measured[path.name] = float(loudness(torch.from_numpy(pcm)[None], RATE))
            key[path.name] = f'{preset}-{version}'
        assert abs(measured[f'{preset}-A.wav']-measured[f'{preset}-B.wav']) < .2
    save(root/'listening-key.json', key)
    save(root/'listening-levels.json', {'seconds': (end-start)/RATE, 'lufs': measured, 'rating': 'pending',
                                      'normalization': 'one fixed gain per complete clip; no window normalization'})
    print(json.dumps(measured))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['prepare', 'summarize', 'candidates', 'listening'])
    parser.add_argument('root', type=Path)
    parser.add_argument('--speech', type=Path)
    parser.add_argument('--presets', type=Path)
    parser.add_argument('--target', type=Path)
    parser.add_argument('--candidate')
    args = parser.parse_args()
    if args.action == 'prepare':
        assert args.speech and args.presets
        prepare(args.root, args.speech, args.presets)
    elif args.action == 'summarize':
        summarize(args.root)
    elif args.action == 'candidates':
        assert args.target
        candidates(args.root, args.target)
    else:
        assert args.candidate
        listening(args.root, args.candidate)
