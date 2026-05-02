export const encodeWAV = (audioBuffer) => {
	const numChannels = 1; // Force mono for Boops
	const sampleRate = audioBuffer.sampleRate;
	const format = 1; // PCM
	const bitDepth = 16;
	
	const samples = audioBuffer.getChannelData(0); 
	const bytesPerSample = bitDepth / 8;
	const blockAlign = numChannels * bytesPerSample;
	
	const buffer = new ArrayBuffer(44 + samples.length * bytesPerSample);
	const view = new DataView(buffer);
	
	const writeString = (view, offset, string) => {
		for (let i = 0; i < string.length; i++) {
			view.setUint8(offset + i, string.charCodeAt(i));
		}
	};

	writeString(view, 0, 'RIFF');
	view.setUint32(4, 36 + samples.length * bytesPerSample, true);
	writeString(view, 8, 'WAVE');
	writeString(view, 12, 'fmt ');
	view.setUint32(16, 16, true);
	view.setUint16(20, format, true);
	view.setUint16(22, numChannels, true);
	view.setUint32(24, sampleRate, true);
	view.setUint32(28, sampleRate * blockAlign, true);
	view.setUint16(32, blockAlign, true);
	view.setUint16(34, bitDepth, true);
	writeString(view, 36, 'data');
	view.setUint32(40, samples.length * bytesPerSample, true);
	
	let offset = 44;
	for (let i = 0; i < samples.length; i++, offset += 2) {
		let s = Math.max(-1, Math.min(1, samples[i]));
		view.setInt16(offset, s < 0 ? s * 0x8000 : s * 0x7FFF, true);
	}
	
	return new Uint8Array(buffer);
};
