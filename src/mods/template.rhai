fn handle(event, context, settings, state) {
    if event == "action:next" {
        return #{commands: [#{action: "next"}]};
    }
    let song = context.playing;
    let caption = if song == () { "Ничего не играет" }
        else { settings.prefix + ": " + song.artist + " — " + song.title };
    if event == "action:show" {
        return #{message: caption, panel: caption};
    }
    #{panel: caption}
}
