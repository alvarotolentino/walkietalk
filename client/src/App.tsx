import { type Component, Match, Switch } from "solid-js";
import { currentScreen, currentParams, Screen } from "./router";
import Splash from "./screens/Splash";
import Login from "./screens/Login";
import Register from "./screens/Register";
import RoomList from "./screens/RoomList";
import CreateRoom from "./screens/CreateRoom";
import JoinByCode from "./screens/JoinByCode";
import PublicRooms from "./screens/PublicRooms";
import RoomView from "./screens/RoomView";
import RoomSettings from "./screens/RoomSettings";
import Profile from "./screens/Profile";

const App: Component = () => {
  return (
    <div class="screen">
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
        <Match when={currentScreen() === Screen.PublicRooms}>
          <PublicRooms />
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
    </div>
  );
};

export default App;
