import { describe, it, expect } from 'vitest';
import { encodeWAV } from './audio';

describe('encodeWAV', () => {
	it('produces a valid WAV header for a mono buffer', () => {
		const sampleRate = 16000;
		const length = 1600; // 0.1s
		const mockBuffer = {
			sampleRate,
			numberOfChannels: 1,
			getChannelData: () => new Float32Array(length).fill(0.5) // Half amplitude
		};
		
		const wavBytes = encodeWAV(mockBuffer);
		
		// Check header
		const view = new DataView(wavBytes.buffer);
		
		// RIFF
		expect(String.fromCharCode(wavBytes[0], wavBytes[1], wavBytes[2], wavBytes[3])).toBe('RIFF');
		// WAVE
		expect(String.fromCharCode(wavBytes[8], wavBytes[9], wavBytes[10], wavBytes[11])).toBe('WAVE');
		// fmt 
		expect(String.fromCharCode(wavBytes[12], wavBytes[13], wavBytes[14], wavBytes[15])).toBe('fmt ');
		// channels (mono = 1)
		expect(view.getUint16(22, true)).toBe(1);
		// sample rate
		expect(view.getUint32(24, true)).toBe(16000);
		// data
		expect(String.fromCharCode(wavBytes[36], wavBytes[37], wavBytes[38], wavBytes[39])).toBe('data');
		
		// Total size = 44 + (1600 * 2 bytes) = 44 + 3200 = 3244
		expect(wavBytes.length).toBe(3244);
		
		// First sample (offset 44) should be 0.5 * 0x7FFF = 16383
		expect(view.getInt16(44, true)).toBe(16383);
	});
});
