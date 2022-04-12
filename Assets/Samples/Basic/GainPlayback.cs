using System;
using UnityEngine;

namespace Wpal.Samples
{
    public class GainPlayback : MonoBehaviour
    {
        private static readonly ushort bytesPerSample = 2; // int16
        private static readonly ushort numChannels = 2;

        [SerializeField] private uint pid = 0;
        [SerializeField, Range(0f, 5f)] private float gain = 1f;

        private AudioRingBuffer buffer = default;
        private byte[] tempBuffer = default;

        private LoopbackCapture loopback = default;

        void Start()
        {
            //the buffer to store captured samples from wpal threads
            buffer = new AudioRingBuffer(
                //100ms
                AudioSettings.outputSampleRate / 10 * bytesPerSample * numChannels,
                bytesPerSample * numChannels);
            StartCapture();
        }

        private void StartCapture()
        {
            loopback = new LoopbackCapture(pid, true, numChannels, (uint)AudioSettings.outputSampleRate,
                (ushort)(bytesPerSample * 8));

            loopback.StartCapture(OnSampleReady);
        }

        private void OnSampleReady(AudioPacket packet)
        {
            buffer.Push(packet);
        }

        private void OnDestroy()
        {
            loopback?.Dispose();
        }

        // play capture samples with gain.
        private void OnAudioFilterRead(float[] data, int channels)
        {
            if (buffer == null) return; // OnAudioFilterRead() runs before assigning of buffer in Start()

            int requiredSamples = data.Length / channels;
            int requiredBytes = bytesPerSample * requiredSamples * numChannels;
            if (tempBuffer == null || tempBuffer.Length < requiredBytes) tempBuffer = new byte[requiredBytes];

            // the format of source buffer is PCM (signed 16-bit little-endian)
            int readBytes = buffer.Read(tempBuffer, 0, requiredBytes);

            // fill samples
            for (int i = 0; i < data.Length; i++)
            {
                int sample = i / channels;
                int channel = i % channels;

                int srcIndex = (sample * numChannels + channel) * bytesPerSample;

                if (channel < numChannels && srcIndex + bytesPerSample < readBytes)
                {
                    data[i] = (float)(BitConverter.ToInt16(tempBuffer, srcIndex)) / 0x8FFF * gain;
                }
                else
                {
                    data[i] = 0; //lack of audio packets or channels
                }
            }
        }
    }
}