import { describe, expect, it } from 'vitest';
import { preserveTextRuns } from '../src/typography/preserve-text-runs.js';
describe('plain text editing preserves canonical font runs', () => {
  it('keeps different fonts across an insertion containing astral characters', () => {
    const a = { fontFamily: 'Lora' }, b = { fontFamily: 'Inter' };
    expect(preserveTextRuns({ text: 'ab😀cd', runs: [{ start:0,end:2,style:a },{start:2,end:5,style:b}] }, 'ab新😀cd')).toEqual({ text:'ab新😀cd', runs:[{start:0,end:3,style:a},{start:3,end:6,style:b}] });
  });
  it('preserves a font after replacing the entire text and handles deletion', () => {
    const body = { text:'abc', runs:[{start:0,end:3,style:{fontFamily:'Lora'}}] };
    expect(preserveTextRuns(body,'新文字😀').runs).toEqual([{start:0,end:4,style:{fontFamily:'Lora'}}]);
    expect(preserveTextRuns(body,'')).toEqual({text:'',runs:[]});
  });
});
