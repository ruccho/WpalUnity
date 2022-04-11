using System;
using System.Runtime.InteropServices;

namespace Wpal
{
    [StructLayout(LayoutKind.Sequential)]
    public struct AudioPacket
    {
        public IntPtr data;
        public uint size;
    }

    public delegate void SampleReadyDelegate(IntPtr capturePtr);

    public class LoopbackCapture : IDisposable
    {
        private static class NativeMethods
        {
            [DllImport("wpal")]
            public static extern IntPtr CreateCapture(uint pid, bool includeProcessTree, ushort numChannels,
                uint numSamplesPerSec, ushort bitsPerSample);

            [DllImport("wpal")]
            public static extern int StartCaptureBlocked(IntPtr capture, SampleReadyDelegate sample_ready_callback);

            [DllImport("wpal")]
            public static extern uint GetNextPacketSize(IntPtr capture);

            [DllImport("wpal")]
            public static extern AudioPacket GetBuffer(IntPtr capture);

            [DllImport("wpal")]
            public static extern void ReleaseBuffer(IntPtr capture, uint frames);

            [DllImport("wpal")]
            public static extern void StopCapture(IntPtr capture);

            [DllImport("wpal")]
            public static extern void DisposeCapture(IntPtr capture);
        }

        public bool IsRunning { get; private set; } = false;

        private IntPtr capturePtr;

        private GCHandle callbackHandle;
        private Action<AudioPacket> onPacketReceived;

        private void LifeCheck()
        {
            if (IsDisposed) throw new ObjectDisposedException(nameof(LoopbackCapture));
        }

        /// <summary>
        /// Create an instance of LoopbackCapture for specified process.
        /// </summary>
        /// <param name="pid">PID of the target process.</param>
        /// <param name="includeProcessTree">Determine capturing behavior. See https://docs.microsoft.com/ja-jp/windows/win32/api/audioclientactivationparams/ne-audioclientactivationparams-process_loopback_mode</param>
        /// <param name="numChannels">Number of channels of output data.</param>
        /// <param name="numSamplesPerSec">Sampling rate of output data.</param>
        /// <param name="bitsPerSample">Depth of samples.</param>
        public LoopbackCapture(uint pid, bool includeProcessTree, ushort numChannels, uint numSamplesPerSec,
            ushort bitsPerSample)
        {
            LifeCheck();
            capturePtr =
                NativeMethods.CreateCapture(pid, includeProcessTree, numChannels, numSamplesPerSec, bitsPerSample);
        }
        
        public bool StartCapture(Action<AudioPacket> onPacketReceived)
        {
            LifeCheck();

            if (IsRunning) return false;

            IsRunning = true;

            SampleReadyDelegate callback = OnSampleReadyCallback;
            callbackHandle = GCHandle.Alloc(callback);

            this.onPacketReceived = onPacketReceived;

            return NativeMethods.StartCaptureBlocked(capturePtr, callback) == 0;
        }

        private void OnSampleReadyCallback(IntPtr capture_ptr)
        {
            if (this == null) return; //Delegate disposed

            if (IsDisposed) return;
            if (!IsRunning) return;
            

            var frames = NativeMethods.GetNextPacketSize(capturePtr);
            if (frames <= 0) return;

            var packet = NativeMethods.GetBuffer(capturePtr);
            
            onPacketReceived?.Invoke(packet);

            NativeMethods.ReleaseBuffer(capturePtr, frames);
        }

        public void StopCapture()
        {
            LifeCheck();
            IsRunning = false;
            NativeMethods.StopCapture(capturePtr);
        }

        public void Dispose()
        {
            InternalDispose();
            GC.SuppressFinalize(this);
        }

        ~LoopbackCapture()
        {
            InternalDispose();
        }

        public bool IsDisposed { get; private set; } = false;

        private void InternalDispose()
        {
            if (IsDisposed) return;

            StopCapture();
            NativeMethods.DisposeCapture(capturePtr);
            capturePtr = IntPtr.Zero;
            if (callbackHandle.IsAllocated) callbackHandle.Free();

            IsDisposed = true;
        }
    }
}