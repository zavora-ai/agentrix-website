import { Composition } from "remotion";
import { Day2Showcase, TOTAL_SECONDS } from "./Composition";
export const RemotionRoot = () => (
  <Composition id="Day2Showcase" component={Day2Showcase} width={1920} height={1080} fps={30} durationInFrames={TOTAL_SECONDS * 30} />
);
