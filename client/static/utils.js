// Utils
export function get(selector, root = document) {
    return root.querySelector(selector);
}
  
export  function formatDate(date) {
    const h = "0" + date.getHours();
    const m = "0" + date.getMinutes();
  
    return `${h.slice(-2)}:${m.slice(-2)}`;
  }

export function formatUTCDate(date){
  date = new Date(date);

  const h = "0" + date.getHours();
  const m = "0" + date.getMinutes();

  return `${h.slice(-2)}:${m.slice(-2)}`;
}
  
export  function random(min, max) {
    return Math.floor(Math.random() * (max - min) + min);
  }
  
  export function insertLineBreaks(text) {
    console.log(text);
    let maxLength = 15;
    let result = '';
    let currentLength = 0;
  
    for (let i = 0; i < text.length; i++) {
      result += text[i];
      currentLength++;
  
      if (currentLength >= maxLength) {
        result += '\n';
        currentLength = 0; // Reset the line length
      }
    }
  
    return result;
  }
  

  export function getRandomString(length) {
    const characters = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789';
    let result = '';
    for (let i = 0; i < length; i++) {
      const randomIndex = Math.floor(Math.random() * characters.length);
      result += characters[randomIndex];
    }
    return result;
  }