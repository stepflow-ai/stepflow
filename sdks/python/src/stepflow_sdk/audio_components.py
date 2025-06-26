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
import datetime

# GUARANTEED DEBUG OUTPUT - This should appear every time the module is loaded
sys.stderr.write("=== AUDIO_COMPONENTS.PY MODULE LOADED ===\n")
sys.stderr.flush()

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
    stream_id: str


class AudioStreamSource:
    """Individual audio source component for generating audio chunks."""
    
    def __init__(self, sample_rate: int = 16000, channels: int = 1, chunk_size: int = 1024, stream_id: str = None):
        self.sample_rate = sample_rate
        self.channels = channels
        self.chunk_size = chunk_size
        self.stream_id = stream_id or str(uuid.uuid4())
    
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
            timestamp=time.time(),
            stream_id=self.stream_id
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
            timestamp=time.time(),
            stream_id=self.stream_id
        )
    
    def start_system_audio_stream(self):
        """Initialize system audio capture (if sounddevice is available)."""
        if not SOUNDDEVICE_AVAILABLE:
            raise ImportError("sounddevice not available for system audio capture")
        
        # Find system audio device
        self.device_info = self._find_system_audio_device()
        if not self.device_info:
            raise RuntimeError("No suitable system audio device found")
        
        log_debug(f"Using system audio device: {self.device_info['name']}", "audio_stream_source")
    
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
            timestamp=time.time(),
            stream_id=self.stream_id
        )


def log_debug(message, component="unknown"):
    """Write debug message to stderr so it shows up in StepFlow logs."""
    timestamp = datetime.datetime.now().strftime("%Y-%m-%d %H:%M:%S")
    sys.stderr.write(f"[{timestamp}] [{component}] {message}\n")
    sys.stderr.flush()  # Ensure it is written immediately


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
    # Test stderr output
    log_debug("TEST: audio_stream_source function called", "audio_stream_source")
    
    log_debug(f"audio_stream_source received input data: {data}", "audio_stream_source")
    
    # Extract parameters from input data
    source_type = data.get('source', 'sine_wave')
    requested_sample_rate = data.get('sample_rate', 44100)
    device_name = data.get('device_name', None)
    duration = data.get('duration', 5.0)
    channels = data.get('channels', 1)
    chunk_size = data.get('chunk_size', 1024)
    frequency = data.get('frequency', 440.0)
    output_file = data.get('output_file', 'output_audio.wav')
    
    log_debug(f"Extracted parameters - source_type: {source_type}, requested_sample_rate: {requested_sample_rate}, device_name: {device_name}", "audio_stream_source")
    
    start_time = time.time()
    
    # Debug: Print the actual input data
    log_debug(f"DEBUG: audio_stream_source received input data: {data}", "audio_stream_source")
    
    stream_id = str(uuid.uuid4())
    
    log_debug(f"TIMING: Starting audio_stream_source at {start_time}", "audio_stream_source")
    
    # Initialize PyAudio
    audio = pyaudio.PyAudio()
    
    # Find device if specified
    device_index = None
    if device_name:
        for i in range(audio.get_device_count()):
            info = audio.get_device_info_by_index(i)
            if device_name.lower() in info['name'].lower():
                device_index = i
                log_debug(f"Found device: {info['name']} (index {i})", "audio_stream_source")
                break
    
    # Auto-detect sample rate if device is specified and requested rate fails
    sample_rate = requested_sample_rate
    if device_index is not None:
        # First try the requested sample rate
        try:
            test_stream = audio.open(format=pyaudio.paInt16,
                                    channels=channels,
                                    rate=requested_sample_rate,
                                    input=True,
                                    input_device_index=device_index,
                                    frames_per_buffer=chunk_size)
            test_stream.close()
            sample_rate = requested_sample_rate
            log_debug(f"Using requested sample rate: {sample_rate} Hz for device", "audio_stream_source")
        except OSError:
            log_debug(f"Requested sample rate {requested_sample_rate} Hz not supported, trying alternatives...", "audio_stream_source")
            # Try alternative sample rates if requested rate fails
            sample_rates = [16000, 22050, 44100, 48000]
            for rate in sample_rates:
                if rate == requested_sample_rate:
                    continue  # Skip the requested rate since it already failed
                try:
                    test_stream = audio.open(format=pyaudio.paInt16,
                                            channels=channels,
                                            rate=rate,
                                            input=True,
                                            input_device_index=device_index,
                                            frames_per_buffer=chunk_size)
                    test_stream.close()
                    sample_rate = rate
                    log_debug(f"Using alternative sample rate: {sample_rate} Hz for device", "audio_stream_source")
                    break
                except OSError:
                    continue
    
    # Calculate how many chunks we need for the full duration
    chunk_duration = chunk_size / sample_rate  # seconds per chunk
    total_chunks = int(duration / chunk_duration)
    
    log_debug(f"DEBUG: Generating {total_chunks} chunks for {duration}s audio at {sample_rate}Hz", "audio_stream_source")
    log_debug(f"DEBUG: chunk_duration={chunk_duration}s, chunk_size={chunk_size} samples", "audio_stream_source")
    log_debug(f"DEBUG: duration={duration}s, sample_rate={sample_rate}Hz", "audio_stream_source")
    log_debug(f"DEBUG: total_chunks calculation: {duration} / {chunk_duration} = {total_chunks}", "audio_stream_source")
    
    # Collect all chunks for direct WAV file writing
    all_chunks = []
    
    if source_type == 'microphone':
        # Set up continuous recording with queue
        audio_queue = queue.Queue()
        recording_stop = threading.Event()
        recording_error = None
        
        def record_audio_continuously():
            """Background thread that continuously records audio."""
            nonlocal recording_error
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
                
                log_debug(f"Started continuous recording from device {device_index}", "audio_stream_source")
                
                chunk_index = 0
                while not recording_stop.is_set() and chunk_index < total_chunks:
                    try:
                        data = stream.read(chunk_size, exception_on_overflow=False)
                        audio_queue.put((chunk_index, data))
                        chunk_index += 1
                        log_debug(f"DEBUG: Recording thread put chunk {chunk_index}/{total_chunks} in queue", "audio_stream_source")
                        if chunk_index % 10 == 0:  # Log every 10th chunk
                            log_debug(f"DEBUG: Recording thread recorded chunk {chunk_index}/{total_chunks}", "audio_stream_source")
                    except Exception as e:
                        log_debug(f"Recording error: {e}", "audio_stream_source")
                        recording_error = e
                        break
                
                stream.stop_stream()
                stream.close()
                log_debug(f"Recording thread completed after {chunk_index} chunks", "audio_stream_source")
                
            except Exception as e:
                log_debug(f"Failed to start recording: {e}", "audio_stream_source")
                recording_error = e
    
        # Start recording thread
        recording_thread = threading.Thread(target=record_audio_continuously, daemon=True)
        recording_thread.start()
        
        log_debug(f"DEBUG: Started continuous recording thread", "audio_stream_source")
        
        # Process chunks from the recording thread
        processed_chunks = 0
        
        while processed_chunks < total_chunks:
            try:
                # Wait indefinitely for chunks - no timeout
                chunk_index, chunk_data = audio_queue.get()
                all_chunks.append((chunk_index, chunk_data))
                
                # Determine if this is the final chunk
                is_final = chunk_index >= total_chunks - 1
                
                log_debug(f"DEBUG: Processing chunk {chunk_index}/{total_chunks}, processed={processed_chunks}, is_final={is_final}", "audio_stream_source")
                
                if is_final:
                    log_debug(f"FINAL_CHUNK: Sending final chunk {chunk_index} with is_final=True", "audio_stream_source")
                
                # Yield the chunk
                yield {
                    "outcome": "streaming",
                    "stream_id": stream_id,
                    "sample_rate": sample_rate,
                    "channels": channels,
                    "chunk_size": chunk_size,
                    "format": "pcm_16bit",
                    "chunk": base64.b64encode(chunk_data).decode('utf-8'),
                    "chunk_index": chunk_index,
                    "is_final": is_final,
                    "output_file": output_file
                }
                
                processed_chunks += 1
                
            except Exception as e:
                log_debug(f"DEBUG: Error processing chunk {processed_chunks}: {e}", "audio_stream_source")
                break
        
        log_debug(f"DEBUG: Main loop completed after processing {processed_chunks} chunks", "audio_stream_source")
        
        # Stop recording
        recording_stop.set()
        recording_thread.join(timeout=2.0)
        
        log_debug(f"DEBUG: Microphone recording loop completed. Processed {len(all_chunks)} chunks out of {total_chunks} expected.", "audio_stream_source")
        
        # Check for any recording errors after completion
        if recording_error:
            raise RuntimeError(f"Microphone recording failed: {recording_error}")
        
    elif source_type == 'system_audio':
        # Similar implementation for system audio
        try:
            source = AudioStreamSource(sample_rate, channels, chunk_size, stream_id=stream_id)
            source.start_system_audio_stream()
            
            for chunk_index in range(total_chunks):
                chunk_start_time = time.time()
                chunk = source.get_system_audio_chunk()
                
                audio_capture_time = time.time()
                log_debug(f"TIMING: System audio capture took {audio_capture_time - chunk_start_time:.4f}s", "audio_stream_source")
                
                all_chunks.append(chunk.data)
                chunk_b64 = base64.b64encode(chunk.data).decode('utf-8')
                is_final = chunk_index >= total_chunks - 1
                
                yield {
                    "outcome": "streaming",
                    "stream_id": stream_id,
                    "sample_rate": chunk.sample_rate,
                    "channels": chunk.channels,
                    "chunk_size": len(chunk.data),
                    "format": "pcm_16bit",
                    "chunk": chunk_b64,
                    "chunk_index": chunk_index,
                    "is_final": is_final,
                    "output_file": output_file
                }
        except Exception as e:
            log_debug(f"System audio capture failed: {e}. Raising error instead of falling back.", "audio_stream_source")
            raise RuntimeError(f"System audio capture failed: {e}")
    
    elif source_type == 'sine_wave':
        # Generate sine wave chunks (no queue needed)
        source = AudioStreamSource(sample_rate, channels, chunk_size)
        
        for chunk_index in range(total_chunks):
            chunk_start_time = time.time()
            chunk = source.generate_sine_wave_chunk(frequency, chunk_duration)
            
            audio_capture_time = time.time()
            log_debug(f"TIMING: Sine wave generation took {audio_capture_time - chunk_start_time:.4f}s", "audio_stream_source")
            
            all_chunks.append(chunk.data)
            chunk_b64 = base64.b64encode(chunk.data).decode('utf-8')
            is_final = chunk_index >= total_chunks - 1
            
            yield {
                "outcome": "streaming",
                "stream_id": stream_id,
                "sample_rate": chunk.sample_rate,
                "channels": chunk.channels,
                "chunk_size": len(chunk.data),
                "format": "pcm_16bit",
                "chunk": chunk_b64,
                "chunk_index": chunk_index,
                "is_final": is_final,
                "output_file": output_file
            }
    else:
        raise ValueError(f"Unsupported audio source type: {source_type}. Supported types: microphone, system_audio, sine_wave")
    
    log_debug(f"DEBUG: Generator loop completed. Processed {len(all_chunks)} chunks.", "audio_stream_source")
    
    # Note: WAV file writing is now handled by the audio_sink component
    # to ensure we write the processed audio, not just the source audio
    
    total_time = time.time() - start_time
    log_debug(f"TIMING: Total audio_stream_source execution took {total_time:.4f}s", "audio_stream_source")


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
    
    # Handle streaming chunk format - extract the actual chunk data
    if 'outcome' in data and data['outcome'] == 'streaming':
        # This is a streaming chunk, extract the chunk data
        chunk_b64 = data.get('chunk', '')
        chunk_index = data.get('chunk_index', 0)
        sample_rate = data.get('sample_rate', 16000)
        channels = data.get('channels', 1)
        stream_id = data.get('stream_id', f'processed_{chunk_index}')
        is_final = data.get('is_final', False)
        operation = data.get('operation', 'passthrough')  # Default operation
        # Pass through output_file from workflow input
        output_file = data.get('output_file', 'output_audio.wav')
    else:
        # Direct input format
        chunk_b64 = data.get('chunk', '')
        chunk_index = data.get('chunk_index', 0)
        sample_rate = data.get('sample_rate', 16000)
        channels = data.get('channels', 1)
        stream_id = data.get('stream_id', f'processed_{chunk_index}')
        is_final = data.get('is_final', False)
        operation = data.get('operation', 'passthrough')
        output_file = data.get('output_file', 'output_audio.wav')
    
    log_debug(f"TIMING: audio_chunk_processor starting chunk {chunk_index} at {start_time}", "audio_chunk_processor")
    log_debug(f"VALIDATION: audio_chunk_processor received chunk_b64 length: {len(chunk_b64)}", "audio_chunk_processor")
    log_debug(f"VALIDATION: audio_chunk_processor operation: {operation}", "audio_chunk_processor")
    
    # Decode base64 chunk
    chunk_data = base64.b64decode(chunk_b64)
    decode_time = time.time()
    log_debug(f"TIMING: Base64 decode took {decode_time - start_time:.4f}s", "audio_chunk_processor")
    log_debug(f"VALIDATION: decoded chunk_data length: {len(chunk_data)} bytes", "audio_chunk_processor")
    
    # Convert bytes to samples
    samples = []
    for i in range(0, len(chunk_data), 2):
        sample = int.from_bytes(chunk_data[i:i+2], 'little', signed=True)
        samples.append(sample)
    
    convert_time = time.time()
    log_debug(f"TIMING: Sample conversion took {convert_time - decode_time:.4f}s", "audio_chunk_processor")
    
    if operation == "amplify":
        # Amplify the audio (multiply by gain factor)
        gain = data.get('gain', 2.0)
        amplified_samples = [int(sample * gain) for sample in samples]
        
        # Convert back to bytes
        amplified_data = b''.join(sample.to_bytes(2, 'little', signed=True) for sample in amplified_samples)
        amplified_b64 = base64.b64encode(amplified_data).decode('utf-8')
        
        process_time = time.time()
        log_debug(f"TIMING: Amplification processing took {process_time - convert_time:.4f}s", "audio_chunk_processor")
        
        result = {
            "outcome": "streaming",
            "stream_id": stream_id,
            "sample_rate": sample_rate,
            "channels": channels,
            "operation": "amplify",
            "gain": gain,
            "chunk": amplified_b64,
            "chunk_index": chunk_index,
            "is_final": is_final,
            "output_file": output_file
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
        log_debug(f"TIMING: Analysis processing took {process_time - convert_time:.4f}s", "audio_chunk_processor")
        
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
            },
            "output_file": output_file
        }
    
    else:
        # Pass through unchanged
        result = {
            "outcome": "streaming",
            "stream_id": stream_id,
            "sample_rate": sample_rate,
            "channels": channels,
            "operation": "passthrough",
            "chunk": chunk_b64,
            "chunk_index": chunk_index,
            "is_final": is_final,
            "output_file": output_file
        }
    
    total_time = time.time() - start_time
    log_debug(f"TIMING: audio_chunk_processor total time for chunk {chunk_index}: {total_time:.4f}s", "audio_chunk_processor")
    
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
    
    # Global storage for accumulating chunks across function calls
    if not hasattr(audio_sink, '_chunk_storage'):
        audio_sink._chunk_storage = {}
    
    # Handle streaming chunk format - extract the actual chunk data
    if 'outcome' in data and data['outcome'] == 'streaming':
        # This is a streaming chunk, extract the chunk data
        chunk_b64 = data.get('chunk', '')
        chunk_index = data.get('chunk_index', 0)
        stream_id = data.get('stream_id', 'unknown')
        sample_rate = data.get('sample_rate', 16000)
        channels = data.get('channels', 1)
        is_final = data.get('is_final', False)
        # For streaming chunks, get output_file from the data (passed from workflow)
        output_file = data.get('output_file', 'output_audio.wav')
        play_audio = data.get('play_audio', False)
    else:
        # Direct input format
        chunk_b64 = data.get('chunk', '')
        chunk_index = data.get('chunk_index', 0)
        stream_id = data.get('stream_id', 'unknown')
        sample_rate = data.get('sample_rate', 16000)
        channels = data.get('channels', 1)
        is_final = data.get('is_final', False)
        output_file = data.get('output_file', 'output_audio.wav')
        play_audio = data.get('play_audio', False)
    
    log_debug(f"TIMING: audio_sink starting chunk {chunk_index} at {start_time}", "audio_sink")
    log_debug(f"DEBUG: audio_sink output_file: {output_file}", "audio_sink")
    log_debug(f"DEBUG: audio_sink is_final: {is_final}", "audio_sink")
    log_debug(f"DEBUG: audio_sink stream_id: {stream_id}", "audio_sink")
    log_debug(f"VALIDATION: audio_sink received chunk_b64 length: {len(chunk_b64)}", "audio_sink")
    
    # Decode the chunk
    if chunk_b64:
        chunk_data = base64.b64decode(chunk_b64)
        decode_time = time.time()
        log_debug(f"TIMING: Base64 decode took {decode_time - start_time:.4f}s", "audio_sink")
        
        # Store the chunk for later writing
        if stream_id not in audio_sink._chunk_storage:
            audio_sink._chunk_storage[stream_id] = {
                'chunks': [],
                'sample_rate': sample_rate,
                'channels': channels,
                'output_file': output_file
            }
        
        audio_sink._chunk_storage[stream_id]['chunks'].append(chunk_data)
        
        log_debug(f"DEBUG: Stored chunk {chunk_index} for stream {stream_id}, total chunks: {len(audio_sink._chunk_storage[stream_id]['chunks'])}", "audio_sink")
        
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
        log_debug(f"TIMING: Audio analysis took {analysis_time - decode_time:.4f}s", "audio_sink")
        
        # Play audio if requested
        if play_audio:
            try:
                import sounddevice as sd
                import numpy as np
                
                # Convert to numpy array
                audio_array = np.array(samples, dtype=np.int16)
                
                # Play the audio
                sd.play(audio_array, samplerate=sample_rate)
                sd.wait()
                
                play_time = time.time()
                log_debug(f"TIMING: Audio playback took {play_time - analysis_time:.4f}s", "audio_sink")
                
            except ImportError:
                log_debug("WARNING: sounddevice not available, skipping audio playback", "audio_sink")
            except Exception as e:
                log_debug(f"ERROR: Audio playback failed: {e}", "audio_sink")
        
        # Write WAV file if this is the final chunk
        if is_final:
            log_debug(f"DEBUG: Final chunk received - checking storage for stream {stream_id}", "audio_sink")
            log_debug(f"DEBUG: Available streams in storage: {list(audio_sink._chunk_storage.keys())}", "audio_sink")
            
            if stream_id in audio_sink._chunk_storage:
                try:
                    log_debug(f"DEBUG: Writing final WAV file: {output_file}", "audio_sink")
                    storage = audio_sink._chunk_storage[stream_id]
                    all_audio_data = b''.join(storage['chunks'])
                    
                    log_debug(f"DEBUG: Total chunks collected: {len(storage['chunks'])}", "audio_sink")
                    log_debug(f"DEBUG: Total audio data size: {len(all_audio_data)} bytes", "audio_sink")
                    
                    # Ensure the output directory exists
                    output_dir = os.path.dirname(output_file)
                    if output_dir and not os.path.exists(output_dir):
                        log_debug(f"DEBUG: Creating output directory: {output_dir}", "audio_sink")
                        os.makedirs(output_dir, exist_ok=True)
                    
                    log_debug(f"DEBUG: Writing {len(all_audio_data)} bytes to {output_file}", "audio_sink")
                    with wave.open(output_file, 'wb') as wav_file:
                        wav_file.setnchannels(storage['channels'])
                        wav_file.setsampwidth(2)  # 16-bit
                        wav_file.setframerate(storage['sample_rate'])
                        wav_file.writeframes(all_audio_data)
                    
                    log_debug(f"SUCCESS: WAV file written successfully: {output_file} ({len(all_audio_data)} bytes)", "audio_sink")
                    
                    # Verify file was created
                    if os.path.exists(output_file):
                        file_size = os.path.getsize(output_file)
                        log_debug(f"SUCCESS: WAV file exists on disk: {output_file} ({file_size} bytes)", "audio_sink")
                    else:
                        log_debug(f"ERROR: WAV file not found on disk after writing: {output_file}", "audio_sink")
                    
                    # Clean up storage for this stream
                    del audio_sink._chunk_storage[stream_id]
                    
                except Exception as e:
                    log_debug(f"ERROR: Failed to write WAV file {output_file}: {e}", "audio_sink")
                    import traceback
                    traceback.print_exc(file=sys.stderr)
            else:
                log_debug(f"ERROR: Stream {stream_id} not found in storage when final chunk received", "audio_sink")
        
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
                "output_file": output_file,
                "is_final": is_final,
                "total_chunks_stored": len(audio_sink._chunk_storage.get(stream_id, {}).get('chunks', []))
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
    log_debug(f"TIMING: audio_sink total time for chunk {chunk_index}: {total_time:.4f}s", "audio_sink")
    
    return result 