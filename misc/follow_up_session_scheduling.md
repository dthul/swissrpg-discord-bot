# Follow up session scheduling

## Currently

- "schedule session"
- Webpage loads info from previous Meetup event
- Form submit triggers creation of new Meetup event
- Hyperion announces new event in channel

## New

- "schedule session"
- Webpage loads info from previous event
- Form submit creates new event in database
- Hyperion announces new event in channel
    - un-sticky old message, sticky new message
    - needs different details
- If event is open for new players, also publish it on Meetup
    - what to do if there is not old Meetup event to copy from? Especially things like location etc.
        - possibly have some default values that work (e.g. default location or online game)

## TODO

X RSVPs: differentiate between RSVPs originating from Meetup or elsewhere (maybe add boolean column(s) to event_participant relation, like "from_meetup", "from_discord" etc.)
X Add a guest limit field for events to the database
- Check Discord sync: channel topic probably needs adjustment
    - can we link to the announcement message instead of the Meetup event?
X Discord sync: sync announcement messages for upcoming events
    - store an (optional) announcement message ID in the event database
    - during sync create or update this message
[
- Can we offer RSVP inside of Discord? -> not for the next release at least
    - Message components
    - Add a button(?) component for RSVPing (use custom_id on button to figure out which event it belongs to)
    - Maybe two buttons, one for RSVPing, one for Un-RSVPing
    - Alternatively a "select menu" for Going vs Not Going. Just not sure if every user sees their own selection state or whether everybody sees whatever the last user selected (which would be confusing). Can it have a pre-selection for each user? Probably not
]
X Do we want to be able to associate a Meetup event with a Hyperion event (possibly via shortcode)? Would there be a use case for that? Which information would be synced from Meetup to Hyperion in that case, only RSVPs?
- what happens (should happen) if event is deleted from Meetup?
X when syncing Meetup, check the database to see if the meetup event ID is already registered (and skip the shortcode stuff?)
    - Hyperion scheduled events don't need any shortcodes then (make sure that missing shortcodes like "online" are not a problem)
    - or maybe stick with the shortcodes for now?
X when scheduling a new session:
    - create the "event" entry in the database
    - if with Meetup event:
        - clone Meetup event, sync it (to get Meetup event ID)
        - will require some changes in the Meetup syncing code. Maybe add an "event" shortcode that refers to the event ID?
        - if error: ?
    - return new event ID and optionally Meetup event ID
    - announce new event (Discord sync)
X make sure flow is deleted once used (to make URL invalid)
- Disable form submit button on click