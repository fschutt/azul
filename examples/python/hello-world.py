from azul import *

css = """
    .__azul-native-label { font-size: 50px; }
"""

class DataModel:
    def __init__(self, counter):
        self.counter = counter

# model -> view
def myLayoutFunc(data, info):
    label = Label.new("{}".format(data.counter))
    button = Button.new("Update counter")
        .withOnClick(data, myOnClick)

    return Dom.body()
        .withChild(label.dom())
        .withChild(button.dom())
        .style(Css.fromString(css))

# model <- view
def myOnClick(data, info):
    data.counter += 1;

    # tell azul to call the myLayoutFunc again
    return Update.RefreshDom()

def main():
    model = DataModel(5)
    app = App.new(model, AppConfig.default())
    app.run(WindowCreateOptions.default())

main()