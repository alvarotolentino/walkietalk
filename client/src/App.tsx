import { type Component, Match, Switch, Show } from "solid-js";
import { currentScreen, Screen } from "./router";
import { isAuthenticated } from "./stores/auth";
import Splash from "./screens/Splash";
import Login from "./screens/Login";
import Register from "./screens/Register";
import RoomList from "./screens/RoomList";
import CreateRoom from "./screens/CreateRoom";
import JoinByCode from "./screens/JoinByCode";
import RoomView from "./screens/RoomView";
import RoomSettings from "./screens/RoomSettings";
import Profile from "./screens/Profile";

const GUEST_SCREENS = new Set([Screen.Splash, Screen.Login, Screen.Register]);

const App: Component = () => {
  const needsAuth = () => !GUEST_SCREENS.has(currentScreen()) && !isAuthenticated();

  return (
    <div class="screen">
      <Show when={!needsAuth()} fallback={<Login />}>
        <Switch>
          <Match when={currentScreen() === Screen.Splash}>
            <Splash />
          </Match>
          <Match when={currentScreen() === Screen.Login}>
            <Login />
          </Match>
          <Match when={currentScreen() === Screen.Register}>
            <Register />
          </Match>
          <Match when={currentScreen() === Screen.RoomList}>
            <RoomList />
          </Match>
          <Match when={currentScreen() === Screen.CreateRoom}>
            <CreateRoom />
          </Match>
          <Match when={currentScreen() === Screen.JoinByCode}>
            <JoinByCode />
          </Match>
          <Match when={currentScreen() === Screen.RoomView}>
            <RoomView />
          </Match>
          <Match when={currentScreen() === Screen.RoomSettings}>
            <RoomSettings />
          </Match>
          <Match when={currentScreen() === Screen.Profile}>
            <Profile />
          </Match>
        </Switch>
      </Show>
    </div>
  );
};

export default App;
