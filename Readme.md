# Flowielytics
This is a work in progress. The application as well as the code are still very much subject to change.

## What is this?
This is a lightweight application that will help you pick the correct runes and buy the correct items for your champion in League of Legends. Instead of providing opinionated in-game overlays, Flowielytics connects to your League of Legends Client and allows you to view statistics for your champion in your browser through 3rd party websites like Lolalytics. The browser tab is kept in sync with your current champion which saves you a lot of typing and clicking.

## Installing
Build from source and run the application using the standard rust tools. Currently only works for windows.

## Will this get me banned?
No, Riot has stated that Vanguard will not ban you for using tools that rely on the LCU: https://www.riotgames.com/en/DevRel/vanguard-faq.

## Tech Stack
The backend of this application is written in Rust, using Axum for incoming requests and tokio-tungstenite for connecting to the LCU. The frontend is server side rendered using htmx and server-sent events. The plumbing between the LCU-websocket and the server-sent events is implemented using a singular tokio watch channel.