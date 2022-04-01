import asyncio
import http.client

HOST = 'localhost:8000'


def increment_counter():
    conn = http.client.HTTPConnection(HOST)
    conn.request('POST', '/')
    r = conn.getresponse()
    assert r.status == 200, r.status


async def main():
    conn = http.client.HTTPConnection(HOST)
    conn.request('GET', '/')
    r = conn.getresponse()
    assert r.status == 200, r.status
    print('before:', int(r.read()))

    loop = asyncio.get_event_loop()
    await asyncio.gather(*[
        loop.run_in_executor(None, increment_counter)
        for _ in range(100)
    ])

    conn = http.client.HTTPConnection(HOST)
    conn.request('GET', '/')
    r = conn.getresponse()
    assert r.status == 200, r.status
    print('after:', int(r.read()))


if __name__ == '__main__':
    asyncio.run(main())
