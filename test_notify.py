"""End-to-end: observer /s, then GET-based iPATCH, then notification."""

import asyncio, aiocoap, cbor2

SERVER = "127.0.0.1"
PORT = 5683
SID = 100029


async def test():
    ctx = await aiocoap.Context.create_client_context()
    notified = 0

    # 1. FETCH+Observe=0 /s
    print("1. FETCH+Observe=0 /s ...", end=" ", flush=True)
    req = aiocoap.Message(code=aiocoap.FETCH)
    req.opt.uri_path = ("s",)
    req.opt.content_format = 141
    req.payload = cbor2.dumps(SID)
    req.opt.observe = 0
    req.unresolved_remote = SERVER
    req.opt.uri_port = PORT

    obs = ctx.request(req)
    first = await obs.response
    print(f"{first.code} observe={first.opt.observe}")

    # 2. Use FETCH (empty payload = full tree) via /c to read, then modify via POST-like.
    #    Actually, let's use the live CLI directly.
    print("2. Triggering change via live CLI...", flush=True)
    import subprocess

    result = subprocess.run(
        [
            "cargo",
            "run",
            "-p",
            "coreconf-cli",
            "--",
            "live",
            "--sid",
            "tutorial/coreconf-m2m@2026-03-29.sid",
            "--server",
            "127.0.0.1:5683",
        ],
        cwd=r"D:\Docs\research\R-SCHC\rust-coreconf",
        input="set /coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/precision 8\npush\nquit\n",
        capture_output=True,
        text=True,
        timeout=10,
    )
    print(f"   CLI: {result.stdout.strip().split(chr(10))[-3:]}")
    if result.stderr:
        for line in result.stderr.strip().split("\n")[-3:]:
            print(f"   ERR: {line}")

    # 3. Wait for notification
    print("3. Waiting for notification (3s)...", flush=True)
    try:
        async with asyncio.timeout(3):
            async for notif in obs.observation:
                notified += 1
                print(
                    f"   NOTIFICATION #{notified}: observe={notif.opt.observe} payload={len(notif.payload)}B"
                )
    except asyncio.TimeoutError:
        pass

    obs.observation.cancel()
    await asyncio.sleep(0.1)
    await ctx.shutdown()

    if notified > 0:
        print(f"\nPASS — {notified} notification(s)")
    else:
        print("\nFAIL — no notifications")


asyncio.run(test())
