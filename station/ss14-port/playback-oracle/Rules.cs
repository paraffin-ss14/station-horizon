using System;
using System.Linq;
using System.Text.Json;
namespace StationOracle;
public static class Rules {
 public static string Advance(string data) {
  Input? x; try{x=JsonSerializer.Deserialize<Input>(data);}catch{return "invalid";}
  if(x==null || x.Delays.Length<2 || x.Delays.Length>64 || x.Delays.Any(v=>!float.IsFinite(v)||v<0.001f||v>60f) || !float.IsFinite(x.Delta)||x.Delta<0||x.Delta>60 || x.Frame<0||x.Frame>=x.Delays.Length || !float.IsFinite(x.Left)||Math.Abs(x.Left)>60) return "invalid";
  var layer=new Layer {AnimationFrame=x.Frame,AnimationTimeLeft=x.Left-x.Delta,AnimationTime=x.Delta,Reversed=x.Reversed,Loop=x.Loop,Cycle=x.Cycle};
  layer.AdvanceFrameAnimation(new RSI.State(x.Delays));
  return JsonSerializer.Serialize(new {frame=layer.AnimationFrame,left=BitConverter.SingleToInt32Bits(layer.AnimationTimeLeft),time=BitConverter.SingleToInt32Bits(layer.AnimationTime),reversed=layer.Reversed,animated=layer.AutoAnimated});
 }
}
public class Input { public float[] Delays {get;set;}=[]; public float Delta {get;set;} public int Frame {get;set;} public float Left {get;set;} public bool Reversed {get;set;} public bool Loop {get;set;} public bool Cycle {get;set;} }
internal class RSI { internal class State(float[] delays) { public int DelayCount=>delays.Length; public float GetDelay(int i)=>delays[i]; } }
internal class Parent {public bool Loop=true;}
internal class Layer {
 internal int AnimationFrame; internal float AnimationTimeLeft, AnimationTime; internal bool Reversed,Loop,Cycle,AutoAnimated=true; internal Parent _parent=new();
            internal void AdvanceFrameAnimation(RSI.State state)
            {
                var delayCount = state.DelayCount;

                while (AnimationTimeLeft < 0)
                {
                    if (Reversed)
                    {
                        AnimationFrame -= 1;

                        if (AnimationFrame < 0)
                        {
                            if (!Loop || !_parent.Loop)
                            {
                                // stop at first frame
                                AnimationFrame = 0;
                                AnimationTimeLeft = 0;
                                AutoAnimated = false;
                                return;
                            }

                            if (Cycle)
                            {
                                AnimationFrame = 1;
                                Reversed = false;
                            }
                            else
                            {
                                AnimationFrame = delayCount - 1;
                            }

                            AnimationTime = -AnimationTimeLeft;
                        }
                    }
                    else
                    {
                        AnimationFrame += 1;

                        if (AnimationFrame >= delayCount)
                        {
                            if (!Loop || !_parent.Loop)
                            {
                                // stop at last frame
                                AnimationFrame = delayCount - 1;
                                AnimationTimeLeft = 0;
                                AutoAnimated = false;
                                return;
                            }

                            if (Cycle)
                            {
                                AnimationFrame = delayCount - 2;
                                Reversed = true;
                            }
                            else
                            {
                                AnimationFrame = 0;
                            }

                            AnimationTime = -AnimationTimeLeft;
                        }
                    }

                    AnimationTimeLeft += state.GetDelay(AnimationFrame);
                }
            }
}
