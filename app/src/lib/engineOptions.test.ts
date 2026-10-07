import { describe, expect, it } from 'vitest';
import type { ManifestOptionView } from './bindings/ManifestOptionView';
import { detailText, fieldOptions, optionError, settingOptions, splitOptions } from './engineOptions';

const option = (over: Partial<ManifestOptionView>): ManifestOptionView => ({
  name: 'hash',
  kind: 'spin',
  default: null,
  min: null,
  max: null,
  choices: [],
  description: null,
  ...over,
});

const threads = option({ name: 'threads', default: '1', min: 1, max: 32 });
const verbose = option({ name: 'verbose', kind: 'check', default: 'false' });
const hash = option({ name: 'hash', min: 1 });
const style = option({ name: 'style', kind: 'combo', default: 'normal', choices: ['normal', 'wild'] });

describe('engine options from a manifest', () => {
  it('leaves buttons out of the settings', () => {
    expect(settingOptions([threads, option({ name: 'clear', kind: 'button' })])).toEqual([threads]);
  });

  it('splits saved options into fields and the rest', () => {
    const { values, other } = splitOptions(
      [
        { name: 'threads', value: '4' },
        { name: 'book', value: 'x.txt' },
      ],
      [threads, verbose],
    );
    expect(values).toEqual({ threads: '4' });
    expect(other).toEqual([{ name: 'book', value: 'x.txt' }]);
  });

  it('saves only fields set to something other than the default', () => {
    const described = [threads, verbose, hash, style];
    expect(fieldOptions(described, { threads: '1', verbose: 'false', hash: '', style: 'normal' })).toEqual([]);
    expect(fieldOptions(described, { threads: '01', verbose: 'true', hash: ' 256 ', style: 'wild' })).toEqual([
      { name: 'verbose', value: 'true' },
      { name: 'hash', value: '256' },
      { name: 'style', value: 'wild' },
    ]);
  });

  it('checks values against the type and bounds', () => {
    expect(optionError(threads, '')).toBeNull();
    expect(optionError(threads, '4')).toBeNull();
    expect(optionError(threads, '1.5')).toBe('must be a whole number');
    expect(optionError(threads, '0')).toBe('must be at least 1');
    expect(optionError(threads, '33')).toBe('must be at most 32');
    expect(optionError(option({ kind: 'float', max: 1 }), '0.75')).toBeNull();
    expect(optionError(option({ kind: 'float', max: 1 }), 'x')).toBe('must be a number');
    expect(optionError(verbose, 'yes')).toBe('must be true or false');
    expect(optionError(style, 'tame')).toBe('must be one of normal, wild');
    expect(optionError(option({ kind: 'string' }), 'any text')).toBeNull();
  });

  it('describes the range and default', () => {
    expect(detailText(threads)).toBe('1 to 32, default 1');
    expect(detailText(hash)).toBe('at least 1');
    expect(detailText(verbose)).toBe('default off');
    expect(detailText(option({ kind: 'check', default: 'true' }))).toBe('default on');
    expect(detailText(style)).toBe('default normal');
  });
});
