<div align="center">

<img src="https://i.imgur.com/iyTwlA0.png" width="20%">
<br>

**tomoyo is an immersion app made in Tauri + SvelteKit. Its goal is to aid you in learning Japanese by mainly just reading native content instead of relying on spaced repetition.**

![Version](https://img.shields.io/badge/version-1.0.0-446db9?style=flat-square)
![Platform](https://img.shields.io/badge/platform-Linux%20%7C%20Windows-446db9?style=flat-square)
![License](https://img.shields.io/badge/license-MIT-lightgrey?style=flat-square)
</div>

<img src="https://i.imgur.com/Mcj4Ghh.png" width="33%"> <img src="https://i.imgur.com/WzSyWJP.png" width="33%"> <img src="https://i.imgur.com/aVfjuU0.png" width="33%">
<img src="https://i.imgur.com/Req3iZI.png" width="33%"> <img src="https://i.imgur.com/SuWlx9k.png" width="33%"> <img src="https://i.imgur.com/0pGgPhN.png" width="33%">


## 📘️ Philosophy
The reason this app came into being is rather simple; while Anki can be helpful in the beginning, it eventually becomes a chore that takes a significant portion of your time. Another issue with it and SRS in general is that it doesn't take into account how frequently you'll encounter particular words and makes you learn them largely without context.

I wanted a tool that allows me to focus more on reading native content rather than grinding flashcards, and tomoyo does exactly that. Instead of spaced repetition, this app bases its learning method on continuously encountering words naturally in context and gradually coming to comprehend them better with repeated exposure. Every mined word has a level assigned to it which you can change on the fly, making each reading session a review in itself. With the option to highlight word levels, as you read more the text will gradually turn more green and show your progress visually.

## 📋️ How to use
Technically all you need is your clipboard, but for something like a game or VN you'll also need something that will extract text from it - either a texthooker or OCR.

Start by creating a media entry: book, article, game, VN, and so on. Once that's done you can enter its page and press the play button to start a reading session - while it's running the app will listen to clipboard changes (or to a websocket) and display text. Click on words to look them up and mine them to your dictionary for the current media entry.

As you mine words they'll start being underlined with their level; click on the underline to change the level on the go.

## ✨️ Features
- Log reading sessions and reading statistics
- Pop-up dictionary
- Track vocabulary coverage for a particular media
- Flashcard review system (non-SRS) for both words and sentences. Review based on word status or media entry
- Mine words together with sentences they appeared in. Words can be mined multiple times to save more sentences containing it
- Fully offline
- Sub-dictionaries for each media entry
- Track commonly looked up words that weren't mined and total look-ups
- Highlight known words based on their level
- Mini mode - resize app to limit it to the text window with adjustable transparency
- Discord Rich Presence
- History - scroll to view previous sentences
- Several themes to choose from
- Text normalization
- Identify conjugation and other grammar forms
- Attach images to words
- Custom name dictionary

## ❓️ FAQ
**Q: Can I use this as a beginner?**  
A: I wouldn't recommend it for a complete beginner; you need a base of both grammar and vocabulary before you start reading native content.

**Q: What if I already have a decent vocabulary base? I don't want to spend time mining words I already know all over again.**  
A: The solution to that is to use the word import feature. I believe you can just copy paste words from your Anki decks that you already know and have them automatically marked as Level 4 (Known).

**Q: How to add custom dictionaries? / Will there be support for more dictionaries**  
A: This is planned in the future.

**Q: How to make the app window stay on top?**  
A: No setting for it currently; use something like Magpie/PowerToys or similar for your specific OS.

**Q: Can this be used for other languages?**  
A: No, and likely never will be. Japanese only.

**Q: Will there be an option to read e-books in the app?**  
A: Maybe in the future, but no guarantees.

**Q: How do I look up only a part of a phrase instead of the longest match?**  
A: Use the cycle function (Shift hotkey by default).

**Q: Where do I report bugs?**  
A: Either create a GH issue or post about it in the **#tomoyo** channel on my [Discord](discord.gg/fHJJm4jpwV)

## ⭐️ Credits/Inspirations
- [JL](https://github.com/rampaa/JL): direct inspiration for the lookup mechanism
- [LingQ](https://www.lingq.com/en/): inspiration for learning philosophy and the word level system
- [JMdict](https://www.edrdg.org/wiki/index.php/JMdict-EDICT_Dictionary_Project): dictionary used in the look-ups
- [VNDB](https://vndb.org/): used as a data source for importing and VN stats
- [絆 Kizuna](https://kizuna-texthooker-ui.app/): inspiration for tracking Japanese immersion
