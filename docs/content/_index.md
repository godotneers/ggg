+++
title = "Godot Goodie Grabber"
description = "Godot Goodie Grabber - the missing project and dependency manager for Godot."
template = "landing.html"

[extra.hero]
title = "Godot Goodie Grabber"
description = "The missing project and dependency manager for Godot."

[[extra.hero.cta_buttons]]
text = "Get Started"
url = "/docs/quick-start/"
style = "primary"

[[extra.hero.cta_buttons]]
text = "View on GitHub"
url = "https://github.com/derkork/ggg"
style = "secondary"

[extra.features_section]
title = "Key Features"
description = "Stop managing Godot versions and addons by hand."

[[extra.features]]
icon = "rocket"
title = "Reproducible Environments"
desc = "Pin the exact Godot version in ggg.toml. Every contributor and every CI run uses the same engine binary, automatically downloaded and cached."

[[extra.features]]
icon = "box"
title = "Git-Based Dependencies"
desc = "Reference addons by git URL, tag, branch, or commit SHA. Godot Goodie Grabber resolves and installs them into your project and locks exact SHAs in ggg.lock."

[[extra.features]]
icon = "code"
title = "Simple CLI"
desc = "ggg sync, ggg edit, ggg run. Familiar commands inspired by uv - no scripts to maintain, no manual steps to document."

[[extra.features]]
icon = "database"
title = "Shared Cache"
desc = "Godot binaries and addon checkouts are stored in a shared cache directory, so multiple projects that share a version never download it twice."
+++
