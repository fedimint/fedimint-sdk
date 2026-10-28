# Agent skills

Skills for people who build with the Fedimint SDK. Each folder is one skill, in the
[Agent Skills](https://agentskills.io) format: a `SKILL.md` with a `name` and a `description`.

| Skill                                                                     | Use it when                                                               |
| ------------------------------------------------------------------------- | ------------------------------------------------------------------------- |
| [`fedimint-react-native-install`](fedimint-react-native-install/SKILL.md) | you add `@fedimint/react-native` to a React Native app for the first time |

Install one in your agent:

```sh
npx skills add fedimint/fedimint-sdk --skill fedimint-react-native-install
```

The skills point at the docs (`docs/`), which stay the source of truth. Skills for people who
work on this repository belong in `.agents/skills/`, not here.
