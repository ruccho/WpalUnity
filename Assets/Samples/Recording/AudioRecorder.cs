using System;
using System.Runtime.InteropServices;
using System.Threading;
using UnityEngine;

namespace Wpal.Samples
{
    public class AudioRecorder : MonoBehaviour
    {
        private static readonly ushort bytesPerSample = 2; // int16
        private static readonly ushort numChannels = 2;

        [SerializeField] private AudioSource audioSource = default;

        private LoopbackCapture loopback = default;
        private bool isRunning = false;

        private int bufferPosition = default;
        private byte[] tempBuffer = default;
        private float[] buffer = default;

        private SynchronizationContext mainThread = default;

        void Start()
        {
            mainThread = SynchronizationContext.Current;
        }

        private string pidText = "0";

        private void OnGUI()
        {
            lock (this)
            {

                GUILayout.BeginHorizontal();
                GUILayout.Label("PID: ");
                pidText = GUILayout.TextField(pidText, GUILayout.MinWidth(40f));

                GUI.enabled = int.TryParse(pidText, out int pid) && pid >= 0 && loopback == null;
                {
                    if (GUILayout.Button("●"))
                    {
                        audioSource.Stop();
                        StartCapture((uint)pid);
                    }
                }
                GUI.enabled = true;

                GUILayout.EndHorizontal();
                
                GUILayout.BeginHorizontal();

                GUI.enabled = loopback == null && audioSource.clip != null;
                {
                    if (!audioSource.isPlaying)
                    {
                        if (GUILayout.Button("▶")) audioSource.Play();
                    }
                    else
                    {
                        if (GUILayout.Button("||")) audioSource.Pause();
                    }

                    if (GUILayout.Button("■")) audioSource.Stop();
                }
                GUI.enabled = true;

                GUILayout.EndHorizontal();

                float position = 0;
                if (buffer != null) position = (float)bufferPosition / buffer.Length;
                if (audioSource.isPlaying) position = audioSource.time / audioSource.clip.length;

                var rect = GUILayoutUtility.GetRect(100f, 20f);

                GUI.DrawTexture(rect, Texture2D.grayTexture);
                rect.width *= position;
                GUI.DrawTexture(rect, Texture2D.whiteTexture);
            }
        }

        private void StartCapture(uint pid)
        {
            buffer = new float[numChannels * AudioSettings.outputSampleRate * 10]; //10 sec
            bufferPosition = 0;

            loopback = new LoopbackCapture(pid, true, numChannels, (uint)AudioSettings.outputSampleRate,
                (ushort)(bytesPerSample * 8));

            isRunning = true;
            loopback.StartCapture(OnSampleReady);
        }

        private void OnSampleReady(AudioPacket packet)
        {
            lock (this)
            {
                if (buffer == null) return;
                if (tempBuffer == null || tempBuffer.Length < packet.size) tempBuffer = new byte[packet.size];

                Marshal.Copy(packet.data, tempBuffer, 0, (int)packet.size);


                int samples = Mathf.Min(buffer.Length - bufferPosition, (int)packet.size / bytesPerSample);

                for (int i = 0; i < samples; i++)
                {
                    buffer[bufferPosition + i] = (float)BitConverter.ToInt16(tempBuffer, i * bytesPerSample) / 0x8FFF;
                }

                bufferPosition += samples;

                if (buffer.Length <= bufferPosition)
                {
                    if (isRunning)
                    {
                        isRunning = false;
                        mainThread.Post(_ => { StopCapture(); }, null);
                    }
                }
            }
        }

        private void StopCapture()
        {
            lock (this)
            {
                loopback?.Dispose();
                loopback = null;
                
                if (buffer == null) return;
                var clip = AudioClip.Create(
                    "Recorded",
                    buffer.Length / numChannels,
                    numChannels,
                    AudioSettings.outputSampleRate,
                    false);


                clip.SetData(buffer, 0);

                audioSource.clip = clip;

                buffer = null;
            }
        }

        private void OnDestroy()
        {
            loopback?.Dispose();
        }
    }
}