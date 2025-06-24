"""
Audio streaming components for Stepflow.
Supports PCM 16-bit audio streaming with base64 encoding.
"""

import base64
import json
import time
import uuid
from typing import Any, Dict, Optional
from dataclasses import dataclass
import pyaudio
import wave
import os
import threading
import queue
import sys
import numpy as np

try:
    import sounddevice as sd
    SOUNDDEVICE_AVAILABLE = True
except ImportError:
    SOUNDDEVICE_AVAILABLE = False


@dataclass
class AudioChunk:
    """Represents a chunk of PCM 16-bit audio data."""
    data: bytes
    sample_rate: int
    channels: int
    chunk_index: int
    timestamp: float


class AudioStreamSource:
    """Individual audio source component for generating audio chunks."""
    
    def __init__(self, sample_rate: int = 16000, channels: int = 1, chunk_size: int = 1024):
        self.sample_rate = sample_rate
        self.channels = channels
        self.chunk_size = chunk_size
        self.stream_id = str(uuid.uuid4())
    
    def start_microphone_stream(self):
        """Initialize microphone stream."""
        self.audio = pyaudio.PyAudio()
        self.stream = self.audio.open(
            format=pyaudio.paInt16,
            channels=self.channels,
            rate=self.sample_rate,
            input=True,
            frames_per_buffer=self.chunk_size
        )
    
    def _record_audio(self):
        """Record a single chunk of audio."""
        return self.stream.read(self.chunk_size, exception_on_overflow=False)
    
    def stop_microphone_stream(self):
        """Stop and clean up microphone stream."""
        if hasattr(self, 'stream'):
            self.stream.stop_stream()
            self.stream.close()
        if hasattr(self, 'audio'):
            self.audio.terminate()
    
    def get_microphone_chunk(self) -> AudioChunk:
        """Get a single chunk of audio from microphone."""
        data = self._record_audio()
        return AudioChunk(
            data=data,
            sample_rate=self.sample_rate,
            channels=self.channels,
            chunk_index=0,
            timestamp=time.time()
        )
    
    def generate_sine_wave_chunk(self, frequency: float = 440.0, duration: float = 0.1) -> AudioChunk:
        """Generate a sine wave chunk for testing."""
        import math
        
        samples = []
        for i in range(self.chunk_size):
            t = i / self.sample_rate
            sample = int(32767 * 0.3 * math.sin(2 * math.pi * frequency * t))
            samples.append(sample)
        
        data = b''.join(sample.to_bytes(2, 'little', signed=True) for sample in samples)
        
        return AudioChunk(
            data=data,
            sample_rate=self.sample_rate,
            channels=self.channels,
            chunk_index=0,
            timestamp=time.time()
        )
    
    def start_system_audio_stream(self):
        """Initialize system audio capture (if sounddevice is available)."""
        if not SOUNDDEVICE_AVAILABLE:
            raise ImportError("sounddevice not available for system audio capture")
        
        # Find system audio device
        self.device_info = self._find_system_audio_device()
        if not self.device_info:
            raise RuntimeError("No suitable system audio device found")
        
        print(f"Using system audio device: {self.device_info['name']}", file=sys.stderr)
    
    def _find_system_audio_device(self):
        """Find a suitable system audio device."""
        devices = sd.query_devices()
        
        # Look for output devices that can be used for loopback
        for device in devices:
            if device['max_inputs'] > 0 and device['max_outputs'] > 0:
                # This device supports both input and output (potential loopback)
                return device
        
        # Fallback to default device
        return sd.query_devices(kind='input')
    
    def get_system_audio_chunk(self):
        """Get a single chunk of system audio."""
        if not SOUNDDEVICE_AVAILABLE:
            raise ImportError("sounddevice not available")
        
        # Record a chunk of system audio
        recording = sd.rec(
            int(self.chunk_size),
            samplerate=self.sample_rate,
            channels=self.channels,
            dtype='int16',
            device=self.device_info['index']
        )
        sd.wait()
        
        # Convert to bytes
        data = recording.tobytes()
        
        return AudioChunk(
            data=data,
            sample_rate=self.sample_rate,
            channels=self.channels,
            chunk_index=0,
            timestamp=time.time()
        )


def audio_stream_source(data: Dict[str, Any], context=None):
    """
    Component that generates audio stream chunks.
    
    Input:
        source: str - audio source type ("sine_wave", "microphone", "system_audio")
        duration: float - duration in seconds
        sample_rate: int - sample rate in Hz (will auto-detect if not supported)
        channels: int - number of audio channels
        chunk_size: int - size of each chunk in samples
        frequency: float - frequency for sine wave (if source is sine_wave)
        output_file: str - path to output WAV file
        device_name: str - name of audio device to use (e.g., "C922 Pro Stream Webcam")
        
    Output:
        Streaming audio chunks with metadata
    """
    import time
    start_time = time.time()
    
    source_type = data.get('source', 'sine_wave')
    duration = data.get('duration', 5.0)
    requested_sample_rate = data.get('sample_rate', 44100)
    channels = data.get('channels', 1)
    chunk_size = data.get('chunk_size', 1024)
    frequency = data.get('frequency', 440.0)
    output_file = data.get('output_file', 'output_audio.wav')
    device_name = data.get('device_name', None)
    
    print(f"TIMING: Starting audio_stream_source at {start_time}", file=sys.stderr)
    
    # Initialize PyAudio
    audio = pyaudio.PyAudio()
    
    # Find device if specified
    device_index = None
    if device_name:
        for i in range(audio.get_device_count()):
            info = audio.get_device_info_by_index(i)
            if device_name.lower() in info['name'].lower():
                device_index = i
                print(f"Found device: {info['name']} (index {i})", file=sys.stderr)
                break
    
    # Auto-detect sample rate if device is specified
    sample_rate = requested_sample_rate
    if device_index is not None:
        sample_rates = [16000, 22050, 44100, 48000]
        for rate in sample_rates:
            try:
                test_stream = audio.open(format=pyaudio.paInt16,
                                        channels=channels,
                                        rate=rate,
                                        input=True,
                                        input_device_index=device_index,
                                        frames_per_buffer=chunk_size)
                test_stream.close()
                sample_rate = rate
                print(f"Using sample rate: {sample_rate} Hz for device", file=sys.stderr)
                break
            except OSError:
                continue
    
    # Calculate how many chunks we need for the full duration
    chunk_duration = chunk_size / sample_rate  # seconds per chunk
    total_chunks = int(duration / chunk_duration)
    
    print(f"DEBUG: Generating {total_chunks} chunks for {duration}s audio at {sample_rate}Hz", file=sys.stderr)
    print(f"DEBUG: chunk_duration={chunk_duration}s, chunk_size={chunk_size} samples", file=sys.stderr)
    
    # Collect all chunks for direct WAV file writing
    all_chunks = []
    
    if source_type == 'microphone':
        # Set up continuous recording with queue
        audio_queue = queue.Queue()
        recording_stop = threading.Event()
        
        def record_audio_continuously():
            """Background thread that continuously records audio."""
            try:
                if device_index is not None:
                    stream = audio.open(
                        format=pyaudio.paInt16,
                        channels=channels,
                        rate=sample_rate,
                        input=True,
                        input_device_index=device_index,
                        frames_per_buffer=chunk_size
                    )
                else:
                    stream = audio.open(
                        format=pyaudio.paInt16,
                        channels=channels,
                        rate=sample_rate,
                        input=True,
                        frames_per_buffer=chunk_size
                    )
                
                print(f"Started continuous recording from device {device_index}", file=sys.stderr)
                
                chunk_index = 0
                while not recording_stop.is_set() and chunk_index < total_chunks:
                    try:
                        data = stream.read(chunk_size, exception_on_overflow=False)
                        audio_queue.put((chunk_index, data))
                        chunk_index += 1
                    except Exception as e:
                        print(f"Recording error: {e}", file=sys.stderr)
                        break
                
                stream.stop_stream()
                stream.close()
                print(f"Recording thread completed after {chunk_index} chunks", file=sys.stderr)
                
            except Exception as e:
                print(f"Failed to start recording: {e}", file=sys.stderr)
    
        # Start recording thread
        recording_thread = threading.Thread(target=record_audio_continuously, daemon=True)
        recording_thread.start()
        
        print(f"DEBUG: Started continuous recording thread", file=sys.stderr)
        
        # Stream chunks as they become available
        for chunk_index in range(total_chunks):
            chunk_start_time = time.time()
            print(f"TIMING: Waiting for chunk {chunk_index}/{total_chunks-1} at {chunk_start_time}", file=sys.stderr)
            
            try:
                # Wait for chunk from recording thread (with timeout)
                received_index, chunk_data = audio_queue.get(timeout=5.0)  # 5 second timeout
                
                if received_index != chunk_index:
                    print(f"WARNING: Expected chunk {chunk_index} but got {received_index}", file=sys.stderr)
                
                audio_capture_time = time.time()
                print(f"TIMING: Got chunk {chunk_index} from queue in {audio_capture_time - chunk_start_time:.4f}s", file=sys.stderr)
                
            except queue.Empty:
                print(f"ERROR: Timeout waiting for chunk {chunk_index}", file=sys.stderr)
                # Generate silence as fallback
                chunk_data = b'\x00' * (chunk_size * 2)  # 16-bit = 2 bytes per sample
            
            # Store chunk for WAV file writing
            all_chunks.append(chunk_data)
            
            chunk_b64 = base64.b64encode(chunk_data).decode('utf-8')
            is_final = chunk_index >= total_chunks - 1
            
            encoding_time = time.time()
            print(f"TIMING: Base64 encoding took {encoding_time - audio_capture_time:.4f}s", file=sys.stderr)
            
            print(f"DEBUG: Yielding chunk {chunk_index}/{total_chunks-1}, is_final={is_final}", file=sys.stderr)
            
            yield {
                "outcome": "streaming",
                "stream_id": str(uuid.uuid4()),
                "sample_rate": sample_rate,
                "channels": channels,
                "chunk_size": len(chunk_data),
                "format": "pcm_16bit",
                "chunk": chunk_b64,
                "chunk_index": chunk_index,
                "is_final": is_final
            }
            
            chunk_end_time = time.time()
            print(f"TIMING: Total chunk {chunk_index} processing took {chunk_end_time - chunk_start_time:.4f}s", file=sys.stderr)
    
        # Stop recording
        recording_stop.set()
        recording_thread.join(timeout=2.0)
        
    elif source_type == 'system_audio':
        # Similar implementation for system audio
        try:
            source = AudioStreamSource(sample_rate, channels, chunk_size)
            source.start_system_audio_stream()
            
            for chunk_index in range(total_chunks):
                chunk_start_time = time.time()
                chunk = source.get_system_audio_chunk()
                
                audio_capture_time = time.time()
                print(f"TIMING: System audio capture took {audio_capture_time - chunk_start_time:.4f}s", file=sys.stderr)
                
                all_chunks.append(chunk.data)
                chunk_b64 = base64.b64encode(chunk.data).decode('utf-8')
                is_final = chunk_index >= total_chunks - 1
                
                yield {
                    "outcome": "streaming",
                    "stream_id": chunk.stream_id,
                    "sample_rate": chunk.sample_rate,
                    "channels": chunk.channels,
                    "chunk_size": len(chunk.data),
                    "format": "pcm_16bit",
                    "chunk": chunk_b64,
                    "chunk_index": chunk_index,
                    "is_final": is_final
                }
        except Exception as e:
            print(f"System audio capture failed: {e}. Falling back to sine wave.", file=sys.stderr)
            source_type = 'sine_wave'
    
    if source_type == 'sine_wave':
        # Generate sine wave chunks (no queue needed)
        source = AudioStreamSource(sample_rate, channels, chunk_size)
        
        for chunk_index in range(total_chunks):
            chunk_start_time = time.time()
            chunk = source.generate_sine_wave_chunk(frequency, chunk_duration)
            
            audio_capture_time = time.time()
            print(f"TIMING: Sine wave generation took {audio_capture_time - chunk_start_time:.4f}s", file=sys.stderr)
            
            all_chunks.append(chunk.data)
            chunk_b64 = base64.b64encode(chunk.data).decode('utf-8')
            is_final = chunk_index >= total_chunks - 1
            
            yield {
                "outcome": "streaming",
                "stream_id": chunk.stream_id,
                "sample_rate": chunk.sample_rate,
                "channels": chunk.channels,
                "chunk_size": len(chunk.data),
                "format": "pcm_16bit",
                "chunk": chunk_b64,
                "chunk_index": chunk_index,
                "is_final": is_final
            }
    
    print(f"DEBUG: Generator loop completed. Processed {len(all_chunks)} chunks.", file=sys.stderr)
    
    # Write WAV file directly when streaming completes
    if all_chunks:
        try:
            print(f"DEBUG: Writing WAV file directly: {output_file}", file=sys.stderr)
            all_audio_data = b''.join(all_chunks)
            
            with wave.open(output_file, 'wb') as wav_file:
                wav_file.setnchannels(channels)
                wav_file.setsampwidth(2)  # 16-bit
                wav_file.setframerate(sample_rate)
                wav_file.writeframes(all_audio_data)
            
            print(f"DEBUG: WAV file written successfully: {output_file} ({len(all_audio_data)} bytes)", file=sys.stderr)
        except Exception as e:
            print(f"ERROR: Failed to write WAV file {output_file}: {e}", file=sys.stderr)
    else:
        print(f"DEBUG: No chunks collected, skipping WAV file write", file=sys.stderr)
    
    total_time = time.time() - start_time
    print(f"TIMING: Total audio_stream_source execution took {total_time:.4f}s", file=sys.stderr)


def audio_chunk_processor(data: Dict[str, Any], context=None) -> Dict[str, Any]:
    """
    Component that processes PCM 16-bit audio chunks.
    
    Input:
        chunk: str - base64 encoded PCM data
        chunk_index: int
        sample_rate: int
        channels: int
        operation: str - processing operation ("amplify", "filter", "analyze")
        
    Output:
        Processed chunk or analysis results
    """
    import time
    start_time = time.time()
    
    chunk_b64 = data['chunk']
    chunk_index = data.get('chunk_index', 0)
    sample_rate = data.get('sample_rate', 16000)
    channels = data.get('channels', 1)
    operation = data.get('operation', 'analyze')
    
    print(f"TIMING: audio_chunk_processor starting chunk {chunk_index} at {start_time}", file=sys.stderr)
    
    # Decode base64 chunk
    chunk_data = base64.b64decode(chunk_b64)
    decode_time = time.time()
    print(f"TIMING: Base64 decode took {decode_time - start_time:.4f}s", file=sys.stderr)
    
    # Convert bytes to samples
    samples = []
    for i in range(0, len(chunk_data), 2):
        sample = int.from_bytes(chunk_data[i:i+2], 'little', signed=True)
        samples.append(sample)
    
    convert_time = time.time()
    print(f"TIMING: Sample conversion took {convert_time - decode_time:.4f}s", file=sys.stderr)
    
    if operation == "amplify":
        # Amplify the audio (multiply by gain factor)
        gain = data.get('gain', 2.0)
        amplified_samples = [int(sample * gain) for sample in samples]
        
        # Convert back to bytes
        amplified_data = b''.join(sample.to_bytes(2, 'little', signed=True) for sample in amplified_samples)
        amplified_b64 = base64.b64encode(amplified_data).decode('utf-8')
        
        process_time = time.time()
        print(f"TIMING: Amplification processing took {process_time - convert_time:.4f}s", file=sys.stderr)
        
        result = {
            "outcome": "streaming",
            "stream_id": data.get('stream_id', f"processed_{chunk_index}"),
            "sample_rate": sample_rate,
            "channels": channels,
            "operation": "amplify",
            "gain": gain,
            "chunk": amplified_b64,
            "chunk_index": chunk_index,
            "is_final": data.get('is_final', False)
        }
    
    elif operation == "analyze":
        # Analyze the audio chunk
        if samples:
            max_amplitude = max(abs(sample) for sample in samples)
            avg_amplitude = sum(abs(sample) for sample in samples) / len(samples)
            rms = (sum(sample * sample for sample in samples) / len(samples)) ** 0.5
        else:
            max_amplitude = avg_amplitude = rms = 0
        
        process_time = time.time()
        print(f"TIMING: Analysis processing took {process_time - convert_time:.4f}s", file=sys.stderr)
        
        result = {
            "outcome": "success",
            "result": {
                "chunk_index": chunk_index,
                "sample_count": len(samples),
                "max_amplitude": max_amplitude,
                "avg_amplitude": avg_amplitude,
                "rms": rms,
                "sample_rate": sample_rate,
                "channels": channels
            }
        }
    
    else:
        # Pass through unchanged
        result = {
            "outcome": "streaming",
            "stream_id": data.get('stream_id', f"passthrough_{chunk_index}"),
            "sample_rate": sample_rate,
            "channels": channels,
            "operation": "passthrough",
            "chunk": chunk_b64,
            "chunk_index": chunk_index,
            "is_final": data.get('is_final', False)
        }
    
    total_time = time.time() - start_time
    print(f"TIMING: audio_chunk_processor total time for chunk {chunk_index}: {total_time:.4f}s", file=sys.stderr)
    
    return result


def audio_sink(data: Dict[str, Any], context=None) -> Dict[str, Any]:
    """
    Component that receives and processes audio chunks (sink).
    
    Input:
        chunk: str - base64 encoded PCM data
        chunk_index: int
        stream_id: str
        output_file: str (optional) - path to output WAV file
        play_audio: bool (optional) - whether to play audio in real time
        
    Output:
        Confirmation of chunk received and file written
    """
    import time
    start_time = time.time()
    
    chunk_b64 = data.get('chunk', '')
    chunk_index = data.get('chunk_index', 0)
    stream_id = data.get('stream_id', 'unknown')
    output_file = data.get('output_file', 'output_audio.wav')
    play_audio = data.get('play_audio', False)
    
    print(f"TIMING: audio_sink starting chunk {chunk_index} at {start_time}", file=sys.stderr)
    
    # Decode the chunk
    if chunk_b64:
        chunk_data = base64.b64decode(chunk_b64)
        decode_time = time.time()
        print(f"TIMING: Base64 decode took {decode_time - start_time:.4f}s", file=sys.stderr)
        
        # Convert to samples for analysis
        samples = []
        for i in range(0, len(chunk_data), 2):
            sample = int.from_bytes(chunk_data[i:i+2], 'little', signed=True)
            samples.append(sample)
        
        # Calculate audio levels
        if samples:
            max_amplitude = max(abs(sample) for sample in samples)
            avg_amplitude = sum(abs(sample) for sample in samples) / len(samples)
            rms = (sum(sample * sample for sample in samples) / len(samples)) ** 0.5
        else:
            max_amplitude = avg_amplitude = rms = 0
        
        analysis_time = time.time()
        print(f"TIMING: Audio analysis took {analysis_time - decode_time:.4f}s", file=sys.stderr)
        
        # Play audio if requested
        if play_audio:
            try:
                import sounddevice as sd
                import numpy as np
                
                # Convert to numpy array
                audio_array = np.array(samples, dtype=np.int16)
                
                # Play the audio
                sd.play(audio_array, samplerate=data.get('sample_rate', 16000))
                sd.wait()
                
                play_time = time.time()
                print(f"TIMING: Audio playback took {play_time - analysis_time:.4f}s", file=sys.stderr)
                
            except ImportError:
                print("WARNING: sounddevice not available, skipping audio playback", file=sys.stderr)
            except Exception as e:
                print(f"ERROR: Audio playback failed: {e}", file=sys.stderr)
        
        result = {
            "outcome": "success",
            "result": {
                "chunk_index": chunk_index,
                "stream_id": stream_id,
                "max_amplitude": max_amplitude,
                "avg_amplitude": avg_amplitude,
                "rms": rms,
                "sample_count": len(samples),
                "chunk_size_bytes": len(chunk_data),
                "output_file": output_file
            }
        }
    else:
        result = {
            "outcome": "success",
            "result": {
                "chunk_index": chunk_index,
                "stream_id": stream_id,
                "message": "No audio data received"
            }
        }
    
    total_time = time.time() - start_time
    print(f"TIMING: audio_sink total time for chunk {chunk_index}: {total_time:.4f}s", file=sys.stderr)
    
    return result 