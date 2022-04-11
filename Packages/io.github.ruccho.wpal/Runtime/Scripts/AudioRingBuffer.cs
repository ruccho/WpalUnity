using System;
using System.Runtime.InteropServices;
using System.Threading;

namespace Wpal
{
    public class AudioRingBuffer //<T> where T : struct
    {
        public enum OverflowBehaviour
        {
            KeepPushing,
            IgnorePushing
        }

        private int BufferBlocks => buffer.Length / alignment;
        private readonly byte[] buffer = default;

        private OverflowBehaviour overflow = OverflowBehaviour.KeepPushing;

        public OverflowBehaviour Overflow
        {
            get
            {
                mut.WaitOne();
                try
                {
                    return overflow;
                }
                finally
                {
                    mut.ReleaseMutex();
                }
            }
            set
            {
                mut.WaitOne();
                try
                {
                    overflow = value;
                }
                finally
                {
                    mut.ReleaseMutex();
                }
            }
        }

        private int pushPosition = 0;
        private int readPosition = 0;

        private readonly int alignment;

        private Mutex mut = new Mutex();

        public AudioRingBuffer(int length, int alignment = 1)
        {
            if (length % alignment != 0) throw new ArgumentException();
            buffer = new byte[length];
            this.alignment = alignment;
        }

        public void Push(AudioPacket packet)
        {
            this.Push(packet.data, 0, (int)packet.size);
        }

        private void Push(IntPtr source, int sourcePosition, int length)
        {
            mut.WaitOne();
            try
            {
                int blocks = length / alignment;
                length = blocks * alignment;
                if (Overflow == OverflowBehaviour.KeepPushing || pushPosition == readPosition)
                {
                    if (BufferBlocks < blocks)
                    {
                        ForwardPushPosition(blocks);
                        sourcePosition += blocks - BufferBlocks;
                        blocks = BufferBlocks;
                    }
                }
                else
                {
                    //IgnorePushing
                    //stop
                    blocks = Math.Min(blocks, BufferBlocks - WroteBlocks - 1);
                }

                int firstHalf = Math.Min(blocks, BufferBlocks - pushPosition);
                int secondHalf = blocks - firstHalf;

                Marshal.Copy(IntPtr.Add(source, sourcePosition * alignment), buffer, pushPosition * alignment,
                    firstHalf * alignment);
                if (secondHalf > 0)
                    Marshal.Copy(IntPtr.Add(source, (sourcePosition + firstHalf) * alignment), buffer, 0,
                        secondHalf * alignment);
                ForwardPushPosition(blocks);
            }
            finally
            {
                mut.ReleaseMutex();
            }
        }

        public int Read(byte[] dest, int destPosition, int length)
        {
            mut.WaitOne();
            try
            {
                int blocks = length / alignment;

                blocks = Math.Min(blocks, WroteBlocks);

                int firstHalf = Math.Min(blocks, BufferBlocks - readPosition);
                int secondHalf = blocks - firstHalf;
                Array.Copy(buffer, readPosition * alignment, dest, destPosition * alignment, firstHalf * alignment);
                if (secondHalf > 0)
                    Array.Copy(buffer, 0, dest, (destPosition + firstHalf) * alignment, secondHalf * alignment);
                readPosition = (readPosition + blocks) % BufferBlocks;
                return blocks * alignment;
            }
            finally
            {
                mut.ReleaseMutex();
            }
        }

        public void Flush()
        {
            mut.WaitOne();
            try
            {
                readPosition = pushPosition;
            }
            finally
            {
                mut.ReleaseMutex();
            }
        }

        private int WroteBlocks => (BufferBlocks + (pushPosition - readPosition)) % BufferBlocks;

        public int LengthAvailable
        {
            get
            {
                mut.WaitOne();
                try
                {
                    return WroteBlocks * alignment;
                }
                finally
                {
                    mut.ReleaseMutex();
                }
            }
        }

        private void ForwardPushPosition(int blocks)
        {
            bool fill = WroteBlocks + blocks >= BufferBlocks;
            pushPosition = (pushPosition + blocks) % BufferBlocks;
            if (fill) readPosition = (BufferBlocks + (pushPosition - 1)) % BufferBlocks;
        }
    }
}