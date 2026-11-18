import { describe, it, expect } from "vitest";
import { activeCues, mediaTime } from "./subtitles";
describe("player subtitles", () => {
  it("preserves overlaps, end exclusivity and positive/negative offset", () => {
    const cues = [{start:1,end:3,text:"一"},{start:2,end:4,text:"二"}];
    expect(activeCues(cues,2.5,0)).toEqual(cues);
    expect(activeCues(cues,3,0)).toEqual([cues[1]]);
    expect(activeCues(cues,1,1)).toEqual([]);
    expect(activeCues(cues,2,1)).toEqual([cues[0]]);
    expect(activeCues(cues,0,-1)).toEqual([cues[0]]);
    expect(activeCues(cues,5,0)).toEqual([]);
  });
  it("formats long audio and invalid clocks", () => {
    expect(mediaTime(3661.3)).toBe("1:01:01"); expect(mediaTime(61)).toBe("1:01");
    expect(mediaTime(NaN)).toBe("0:00"); expect(mediaTime(-1)).toBe("0:00");
  });
});
