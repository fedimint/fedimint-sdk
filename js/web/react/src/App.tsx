import React from 'react'
import './App.css'
import HooksDemo from './components/HooksDemo'
// @ts-ignore
import reactImage from './assets/react.svg'

function App() {
  return (
    <div className="App">
      <img height={200} src={reactImage} />
      <h2>Fedimint Sdk - React</h2>
      <HooksDemo />
    </div>
  )
}

export default App
