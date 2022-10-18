from gql import gql, Client
from gql.transport.requests import RequestsHTTPTransport
import redis
import time

with open("../lib/src/meetup/schema.graphql") as f:
    schema_str = f.read()

transport = RequestsHTTPTransport(
    url="https://api.meetup.com/gql",
    headers={"Authorization": "Bearer access_token_goes_here"},
)
client = Client(schema=schema_str, transport=transport)

query = gql(
    """
    query EventDescriptionQuery($eventId: ID!) {
        event(id: $eventId) {
            description
        }
    }
    """
)

r0 = redis.Redis(host="localhost", port=6380, db=0)
r1 = redis.Redis(host="localhost", port=6380, db=1)

event_ids = [id.decode("utf8") for id in r0.smembers("meetup_events")]

for i, event_id in enumerate(event_ids):
    print(f"\r{i+1} / {len(event_ids)}", end="", flush=True)
    description = r0.get(f"meetup_event:{event_id}:description")
    if description is not None:
        continue
    if description is None:
        description = r1.get(f"meetup_event:{event_id}:description")
    if description is not None:
        r0.set(f"meetup_event:{event_id}:description", description)
        continue
    if description is None:
        params = {"eventId": event_id}
        time.sleep(0.25)
        result = client.execute(query, variable_values=params)
        if "event" in result and "description" in result["event"]:
            description = result["event"]["description"]
            r0.set(f"meetup_event:{event_id}:description", description)
print()
