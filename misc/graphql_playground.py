from gql import gql, Client
from gql.transport.requests import RequestsHTTPTransport
import sys

with open("lib/src/meetup/schema.graphql") as f:
    schema_str = f.read()

transport = RequestsHTTPTransport(
    url="https://api.meetup.com/gql",
    headers={"Authorization": "Bearer 02e1dbc04690c12ca0f6abafc954264f"},
)
client = Client(schema=schema_str, transport=transport)

query = gql(
    """
    query UpcomingEventsQuery($urlname: String!) {
        groupByUrlname(urlname: $urlname) {
            upcomingEvents(input: {first: 100}) {
            pageInfo {
                hasNextPage
                endCursor
            }
            count
            edges {
                node {
                id
                title
                eventUrl
                shortUrl
                shortDescription
                description
                hosts {
                    id
                }
                dateTime
                maxTickets
                going
                isOnline
                rsvpSettings {
                    rsvpsClosed
                }
                venue {
                    lat
                    lng
                }
                group {
                    urlname
                }
                }
            }
            }
        }
    }
    """
)
params = {"urlname": "SwissRPG-Zurich"}
result = client.execute(query, variable_values=params)
print(result)
sys.exit(0)

create_event_mutation = gql(
    """
    mutation CreateEventMutation($input: CreateEventInput!) {
        createEvent(input: $input) {
            event {
                id
                title
                eventUrl
                shortUrl
                description
                hosts {
                    id
                }
                dateTime
                maxTickets
                going
                isOnline
                rsvpSettings {
                    rsvpsClosed
                }
                venue {
                    lat
                    lng
                }
                group {
                    urlname
                }
                }
                errors {
                code
                message
                field
            }
        }
    }
"""
)
# create_event_input = {
#     "groupUrlname": "SwissRPG-Romandie",
#     "title": "Test event",
#     "description": "Test description",
#     "startDateTime": "2022-01-31T17:30:00",
#     "duration": "PT14400S",
#     "rsvpSettings": {
#         "rsvpLimit": 6,
#         "guestLimit": 0,
#     },
#     "eventHosts": [336046718],
#     "venueId": "27185993",
#     "selfRsvp": False,
#     "howToFindUs": "Specified on Discord",
#     "featuredPhotoId": 499133499,
#     "publishStatus": "PUBLISHED",
# }
create_event_input = {
    "groupUrlname": "SwissRPG-Zurich",
    "title": "Test Event",
    "description": "Test description",
    "startDateTime": "2022-01-31T17:30:00+00:00",
}
result = client.execute(
    create_event_mutation, variable_values={"input": create_event_input}
)
result
