# Overall
The pluton server manages files through two systems, the database and the http server. Users upload files to the HTTP servers, checks are done, then the file is saved to the database to be referenced. Each file is given a unique ID that it can be referenced by.

# The process
This uses an example of a user wanting to upload an image file (.png). The user first contacts the server's HTTP server, requesting to upload a file alongside the image's contents and the session ID (authentication). THe server saves the file to the database. The HTTP response contains the image ID. The user uploads this image when sending a message. Internally, the message might look something like:
"
<#a43g2bji90ejneo30gueouhneiu==>
hi check this cool picture out ok bye
"
Clients that see this would request that image ID from the server and display it above the message.
