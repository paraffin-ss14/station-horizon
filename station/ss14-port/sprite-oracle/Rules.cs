// Copyright (c) 2019 Space Station 14 Contributors. MIT: ../ROBUST-LICENSE.txt.
using System.Text.Json;
namespace StationOracle;
public static class Rules {
    // JSON adapter validates bounded input, then runs the original FoldDelays.
    // Float output encoded as IEEE bits to compare every original f32 exactly.
    public static string Fold(string data) {
        float[][]? input;
        try {input=JsonSerializer.Deserialize<float[][]>(data);} catch {return "invalid";}
        if(input==null || !(input.Length==1 || input.Length==4 || input.Length==8)
           || input.Any(d=>d==null || d.Length==0 || d.Length>64 || d.Any(v=>!float.IsFinite(v) || v<0.001f || v>60f))) return "invalid";
        var result=FoldDelays(input);
        return JsonSerializer.Serialize(new { delays=result.delays.Select(BitConverter.SingleToInt32Bits).ToArray(), indices=result.indices });
    }
        private static (float[] delays, int[][] indices) FoldDelays(float[][] delays)
        {
            if (delays.Length == 1)
            {
                // Short circuit handle single directional sprites.
                var delayList = delays[0];
                var output = new float[delayList.Length];
                var indices = new int[delayList.Length];

                for (var i = 0; i < delayList.Length; i++)
                {
                    output[i] = delayList[i];
                    indices[i] = i;
                }

                return (output, new[] {indices});
            }

            // Multiply by 1000 so we have millisecond precision in our fixed point.
            const float fixedPointResolution = 1000;

            var dirCount = delays.Length;

            // Convert to int[][] to use our fixed point and avoid floating point pains.
            // Also we mutate these arrays to make the calculations easier.
            // We also calculate the lengths for later.
            var iDelays = new int[dirCount][];
            Span<int> dirLengths = stackalloc int[dirCount];
            var maxLength = 0;

            for (var d = 0; d < dirCount; d++)
            {
                var length = 0;
                var fDelayList = delays[d];
                var delayList = new int[fDelayList.Length];
                iDelays[d] = delayList;

                for (var i = 0; i < delayList.Length; i++)
                {
                    var delay = (int) (fDelayList[i] * fixedPointResolution);
                    delayList[i] = delay;
                    length += delay;
                }

                maxLength = Math.Max(length, maxLength);
                dirLengths[d] = length;
            }

            // Extend final delay so that all sets have the same total length.
            // Strictly speaking this shouldn't be necessary, since the RSI spec mandates equal lengths.
            // But better safe than sorry, especially if there's some funky floating point conversions.
            for (var d = 0; d < dirCount; d++)
            {
                var length = dirLengths[d];
                var diff = maxLength - length;

                iDelays[d][^1] += diff;
            }

            // Calculate base texture indices for the directions.
            Span<int> dirIndexOffsets = stackalloc int[dirCount];
            dirIndexOffsets.Fill(0);

            for (var i = 0; i < dirCount - 1; i++)
            {
                dirIndexOffsets[i + 1] = dirIndexOffsets[i] + delays[i].Length;
            }

            // Offsets in each directions array, since we don't go through each with the same index.
            Span<int> dirDelayOffsets = stackalloc int[dirCount];
            dirDelayOffsets.Fill(0);

            // Output delays list.
            var newDelays = new List<int>();

            // Output indices list.
            var newIndices = new List<int>[dirCount];
            for (var d = 0; d < dirCount; d++)
            {
                newIndices[d] = new List<int>();
            }

            // Actually get churning through these delays.
            while (true)
            {
                var minDelay = int.MaxValue;

                // Calculate the minimum delay for each direction we're currently at.
                for (var d = 0; d < dirCount; d++)
                {
                    var o = dirDelayOffsets[d];
                    var delay = iDelays[d][o];

                    minDelay = Math.Min(delay, minDelay);

                    // This will obviously be a frame, so write the index for the texture into output indices.
                    newIndices[d].Add(dirIndexOffsets[d] + o);
                }

                // Add said minimum delay to the output delays.
                newDelays.Add(minDelay);

                for (var d = 0; d < dirCount; d++)
                {
                    ref var o = ref dirDelayOffsets[d];
                    ref var delay = ref iDelays[d][o];

                    // Subtract working delays.
                    delay -= minDelay;

                    if (delay == 0)
                    {
                        // Increment offset in array for the direction(s) that fully completed a frame this iteration.
                        o += 1;
                    }

                    // We've reached the end of one direction.
                    // Since every direction has the same length, this *must* mean we're done.
                    if (o == iDelays[d].Length)
                    {
                        goto done;
                    }
                }
            }

            done:

            // Turn output data into a format suitable for returning and we're done.
            var floatDelays = new float[newDelays.Count];

            for (var i = 0; i < newDelays.Count; i++)
            {
                floatDelays[i] = newDelays[i] / fixedPointResolution;
            }

            var arrayIndices = new int[dirCount][];
            for (var d = 0; d < dirCount; d++)
            {
                arrayIndices[d] = newIndices[d].ToArray();
            }

            return (floatDelays, arrayIndices);
        }
}
