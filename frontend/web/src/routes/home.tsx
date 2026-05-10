import { StubRoute } from "./_stub";

export function HomeRoute() {
  return (
    <StubRoute
      title="Home"
      sub="Mission control · paper · localhost"
      body={
        <>
          Live equity, recent decisions, and the agent journal will land here
          once the eval engine and chat rail backends are wired up.
        </>
      }
    />
  );
}
